# Shell Universal (Mizu Shell vs Personalidades)

> Mizu = microkernel orquestrador de ABIs com foco em produção.
> TipOS = sistema mínimo para programação (TUI, graphy, compila em si mesmo).
> O Mizu não copia o TipOS; usa a ideia (syscall Linux) em Rust puro e com foco distinto.

## Decisão (esta madrugada)

- **Shell do sistema = binário nativo Mizu** (Rust, personalidade `NATIVE`, tabela de syscalls do Mizu), em ring 3 como processo (alvo: carregar como ELF na initramfs igual ao `init.elf`), não como shell do kernel em ring 0.
- **Cada camada usa as suas ferramentas:** Linux usa `bash/zsh` dele via personality Linux, Windows usa `powershell/cmd` via Win32, Solaris usa `zsh/ksh`, DOS usa `COMMAND.COM`. O shell Mizu só despacha (`linux! bash -i`, `win! powershell`) e importam/exportam via FDs e pipes do Mizu.

## Por que não adotar Nushell agora

- Nushell é ELF Linux grande e exige a ABI Linux quase completa (`openat`, `clone`, `statx` etc.).
- Rodar Nushell hoje seria rodar um ELF Linux no Mizu conhecido e esperar que funcione, em vez de focar no orquestrador.
- Quando a ABI Linux completar, o Nushell entra como programa comum em ring 3, sem ser o shell do microkernel.

## Caminho barato (agora)

Estender `mizu-sh` + `mizu-core` (já existe REPL, pipes, history, autocomplete):

1. **IR comum** (pipeline com dados estruturados, estilo Nushell): `exec`, `pipe`, `redir`, `var`, `personality`.
2. **Dialetos como afinadores de parser:**
   - `bash`: `$VAR`, `$(cmd)`, `"..."`, `'...'`, `| > >> <`
   - `zsh`: glob `**`, `=(cmd)`, `autoload` (subset)
   - `powershell`: `Get-X | Where-Object { }`, `$env:`, `;`
3. **Exec por magic number** (`detect_abi` já esboçado em `kernel_schema.md`):
   - `0x7F454C46` (ELF) → personality Linux
   - `MZ` (`0x5A4D`) → DOS/Win32 (PE vs DOS depois)
   - `0xCAFEBABE` → Java, `WASM` magic → WASM
4. **Hako vs Rust:** shell é Rust. Hako ainda incompleta (não retorna valor em `inb`, etc.); drivers e shell ficam em Rust puro até o Hako ficar maduro.

## Próximos passos (ordem)

1. Validar `readv/writev` (145/146) em QEMU com `fread` musl.
2. `openat` (295) / `fstatat` para o musl moderno.
3. `clone(120)` para threads/`posix_spawn`.
4. Tirar `sti` tardio do papel: confirmar sem hang em QEMU com PIT 100 Hz + teclado IRQ.
