# Hako — Linguagem Hierárquica para Hardware

Hako é uma linguagem de domínio específico (DSL) para escrever drivers e componentes de kernel que transpila para Rust. Projetada para ser concisa, legível e dar controle total sobre hardware via `unsafe { core::arch::asm!() }`.

## Índice

1. [Sintaxe da Linguagem](#sintaxe-da-linguagem)
   - [Box](#box)
   - [Itens](#itens)
   - [Funções](#funções)
   - [Modos de Função](#modos-de-função)
   - [Flow](#flow)
   - [Statements](#statements)
   - [Expressões](#expressões)
2. [Stdlib Predefinida](#stdlib-predefinida)
3. [Transpilador](#transpilador)
4. [Integração com Kernel](#integração-com-kernel)

---

## Sintaxe da Linguagem

### Box

Um `box` agrupa constantes e funções em um módulo Rust:

```hako
box serial {
  COM1 = 0x3F8
  COM2 = 0x2F8

  config => default
  write_byte(b) => default
  read_byte() => default
}
```

Boxes podem estender outros boxes com `::`:

```hako
box my_serial::serial {
  // herda constantes e funções de 'serial'
}
```

### Itens

Dentro de um box, três tipos de item:

| Sintaxe | Descrição | Geração Rust |
|---------|-----------|--------------|
| `NOME = VALOR` | Constante | `pub const NOME: u32 = VALOR;` |
| `NOME(args) MODO { corpo }` | Função | `pub fn NOME(args) { ... }` |
| `NOME(args) => default` | Auto-mode (stdlib) | Chama `hako_stdlib::*()` |
| `expressão;` | Side-effect | `// side-effect: expressão` |

### Funções

Parâmetros com tipagem explícita opcional:

```hako
// Inferência: s/msg/str → &str, data/buf/buffer/arr → &[u32], resto → u32
greet(s) { vga::write_str(s) }

// Tipos explícitos
read_sectors(lba: u32, count: u32, buf: &[u16]) {
  // implementação
}
```

Sem parâmetros, os parênteses são opcionais:

```hako
run {          // OK: sem parênteses
  clear()
  write_str("hello")
}

run() raw {    // OK: com parênteses + raw
  out(al, result)
  "in al, dx"
}
```

### Modos de Função

Três modos:

#### `=> default` (Auto)

Mapeia para implementações predefinidas na stdlib:

```hako
config => default                     // → hako_stdlib::serial_config(COM1)
write_byte(b) => default              // → hako_stdlib::serial_write_byte(b, COM1)
clear => default                      // → hako_stdlib::vga_clear()
write_str(s) => default               // → hako_stdlib::vga_write_str(s)
```

Constantes do box são injetadas como argumentos extras automaticamente.

#### `{ ... }` (Block)

Código Rust estruturado com statements Hako:

```hako
fib(n) {
  if n <= 1 => { n }
  else { fib(n - 1) + fib(n - 2) }
}
```

#### `raw { ... }` (Raw/Inline Assembly)

Bloco de assembly inline com suporte a operandos `out()` e `in()`:

```hako
read_byte(port) raw {
  in(dx, port)              // input: dx recebe 'port' (com cast u16)
  out(al, result)           // output: al salvo em 'result' (tipo u8)
  "in al, dx"               // template assembly
}
```

Gera:

```rust
fn read_byte(port: u32) {
    let mut __out_result: u8;
    unsafe {
        core::arch::asm!(
            "in al, dx",
            in("dx") port as u16,
            out("al") __out_result
        );
    }
    let result = __out_result;
}
```

**Regras para `out(reg, var)`**:
- Gera `let mut __out_var: T` (T inferido do registro)
- Gera `out("reg") __out_var` no asm!
- Gera `let var = __out_var;` após o bloco

**Regras para `in(reg, var)`**:
- Gera `in("reg") var as T` com cast automático

**Mapeamento registro → tipo**:

| Registro | Tipo |
|----------|------|
| `al`, `ah`, `bl`, `bh`, `cl`, `ch`, `dl`, `dh` | `u8` |
| `ax`, `bx`, `cx`, `dx`, `si`, `di`, `sp`, `bp` | `u16` |
| outros (eax, ebx, ecx, edx, ...) | `u32` |

### Flow

Define ordem de execução entre boxes. Cada passo chama `<box>::run()`:

```hako
flow default {
  serial    // → serial::run()
  vga       // → vga::run()
  process   // → process::run()
}
```

Gera:

```rust
pub fn flow_default() {
    serial::run();
    vga::run();
    process::run();
}
```

### Statements

| Sintaxe Hako | Geração Rust |
|---|---|
| `if COND => { ... } else { ... }` | `if COND { ... } else { ... }` |
| `loop { ... }` | `loop { ... }` |
| `for VAR in EXPR { ... }` | `for VAR in EXPR { ... }` |
| `break` | `break;` |
| `VAR = EXPR` | `VAR = EXPR;` |
| `EXPR` | `EXPR;` |
| `{ ... }` | `{ ... }` (bloco aninhado) |
| `out(REG, VAR)` | (ver raw mode acima) |
| `in(REG, VAR)` | (ver raw mode acima) |
| `"asm line"` | (ver raw mode acima) |

**Array indexing**: lvalues com indexação são suportados em atribuições:

```hako
buf[i] = 42           // → buf[i] = 42;
x = buf[i]            // → x = buf[i];
vec[idx + 1] = x      // → vec[idx + 1] = x;
```

### Expressões

Expressões são passadas como texto para Rust. Qualquer expressão Rust válida funciona:

```hako
x = (a + b) * 3 / foo()
buf[i + 1] = arr[j] | mask
result = func(a, b + 1)
```

---

## Stdlib Predefinida

Injetada como `pub mod hako_stdlib {}` no código gerado.

### Port I/O

```rust
fn port_outb(value: u32, port: u32)   // out dx, al
fn port_inb(port: u32) -> u32          // in al, dx
```

### Serial (COM)

```rust
pub fn serial_config(com: u32)
pub fn serial_write_byte(b: u32, com: u32)
pub fn serial_read_byte(com: u32) -> u32
```

### VGA Text Mode

```rust
pub fn vga_put_char(c: u32, x: u32, y: u32)
pub fn vga_write_str(s: &str)
pub fn vga_clear()
pub fn vga_scroll()
pub fn vga_set_cursor(x: u32, y: u32)
```

### PIT

```rust
pub fn pit_config(freq: u32)
```

### Keyboard

```rust
pub fn keyboard_init()
```

### Mapeamento Auto-mode → Stdlib

| Nome função | Stdlib chamada | Args extras |
|---|---|---|
| `config`, `setup` | `serial_config` | `COM1` |
| `write_byte` | `serial_write_byte` | `COM1` |
| `read_byte` | `serial_read_byte` | `COM1` |
| `put_char` | `vga_put_char` | — |
| `write`, `write_str` | `vga_write_str` | — |
| `clear` | `vga_clear` | — |
| `scroll` | `vga_scroll` | — |
| `set_cursor` | `vga_set_cursor` | — |
| `outb` | `port_outb` | — |
| `inb` | `port_inb` | — |
| `init` | `keyboard_init` | — |

---

## Transpilador

### Pipeline

```
.hako → [Parser] → AST → [Codegen] → .rs
         ↓                    ↓
    parser.rs             codegen.rs
         ↓                    ↓
    ast.rs               + stdlib.rs (injetado)
```

### API

```rust
// Lib
hako::transpile_file(input: &Path, output: &Path)
    -> Result<(box_count, impl_count), String>

// CLI
hako input.hako [-o output.rs]
```

### Arquivos

| Arquivo | Descrição |
|---|---|
| `ast.rs` | Tipos: `Program`, `Box`, `Item`, `Stmt`, `FnMode`, `Flow` |
| `parser.rs` | `Parser` — recursivo descendente, `skip()` ignora whitespace/comentários |
| `codegen.rs` | `Codegen` — geração Rust com `emit_fn()`, `emit_raw_block()`, `stmt_str()` |
| `stdlib.rs` | `pub const STDLIB: &str` — fonte Rust injetado como módulo |
| `lib.rs` | `transpile_file()` — orquestra parser → codegen |
| `main.rs` | CLI `hako <input> [-o <output>]` |

### Novos Statements (Raw Mode)

Adicionados ao enum `Stmt`:

```rust
pub enum Stmt {
    // ... existentes ...
    AsmOut { reg: String, var: String },   // out(al, result)
    AsmIn  { reg: String, var: String },   // in(dx, port)
    AsmLine(String),                       // "in al, dx"
}
```

### Type Inference

```rust
fn infer_type(name: &str) -> &str {
    match name {
        "s" | "msg" | "str" => "&str",
        "data" | "buf" | "buffer" | "arr" => "&[u32]",
        _ => "u32",
    }
}
```

Tipos explícitos (sintaxe `nome: tipo`) sobrescrevem a inferência.

---

## Integração com Kernel

No `build.rs` do kernel Mizu:

```rust
// Para cada .hako em src/hako/:
hako::transpile_file(&path, &hako_gen_dir.join(format!("{}.rs", stem)))?;

// mod.rs include! todos os arquivos gerados
// main.rs: include!(concat!(env!("OUT_DIR"), "/hako_gen/mod.rs"));
```

O kernel chama `flow_default()` em `kmain()`:

```rust
kprintln!("  hako: running...");
flow_default();
kprintln!("  hako: OK");
```

### Demo (`mizu-kernel/src/hako/demo.hako`)

```hako
box serial {
  COM1 = 0x3F8;  COM2 = 0x2F8
  config => default;  write_byte(b) => default
  run { config();  write_byte(0x48) ... }
}
box vga {
  clear => default;  write_str(s) => default
  run { clear();  write_str("...") }
}
box entry { greet(s) { vga::write_str(s) } }
box output { run { entry::greet("...") } }
box process { run { serial::run();  vga::run();  output::run() } }
flow default { process }
```
