use std::fmt;

#[derive(Debug, Clone)]
pub struct HakoProgram {
    pub boxes: Vec<HakoBox>,
    pub impls: Vec<HakoImpl>,
}

#[derive(Debug, Clone)]
pub struct HakoBox {
    pub name: String,
    pub instructions: Vec<HakoInstDecl>,
}

#[derive(Debug, Clone)]
pub struct HakoInstDecl {
    pub name: String,
    pub params: Vec<HakoParam>,
    pub ret_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HakoParam {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone)]
pub struct HakoImpl {
    pub box_name: String,
    pub inst_name: String,
    pub params: Vec<HakoParam>,
    pub ret_type: Option<String>,
    pub body: Vec<HakoStmt>,
}

#[derive(Debug, Clone)]
pub enum HakoStmt {
    Let {
        name: String,
        type_name: Option<String>,
        is_mut: bool,
        value: Option<String>,
    },
    Expr(String),
    Caso {
        cond: String,
        body: Vec<HakoStmt>,
    },
    CasoContrario(Vec<HakoStmt>),
    Retorna(Option<String>),
    Asm {
        template: String,
        operands: Vec<String>,
        clobbers: Vec<String>,
    },
    Loop(Vec<HakoStmt>),
    For {
        var: String,
        iter: String,
        body: Vec<HakoStmt>,
    },
    Block(Vec<HakoStmt>),
    Ref(String),
    Empty,
}

impl fmt::Display for HakoProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in &self.boxes {
            writeln!(f, "box {} {{", b.name)?;
            for inst in &b.instructions {
                write!(f, "  inst {}(", inst.name)?;
                for (i, p) in inst.params.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", p.name, p.type_name)?;
                }
                write!(f, ")")?;
                if let Some(ref rt) = inst.ret_type {
                    write!(f, " -> {}", rt)?;
                }
                writeln!(f)?;
            }
            writeln!(f, "}}")?;
        }
        for imp in &self.impls {
            writeln!(f)?;
            write!(f, "impl {}::{}(", imp.box_name, imp.inst_name)?;
            for (i, p) in imp.params.iter().enumerate() {
                if i > 0 { write!(f, ", ")?; }
                write!(f, "{}: {}", p.name, p.type_name)?;
            }
            write!(f, ")")?;
            if let Some(ref rt) = imp.ret_type {
                write!(f, " -> {}", rt)?;
            }
            writeln!(f, " {{")?;
            for stmt in &imp.body {
                write_stmt(f, stmt, 1)?;
            }
            writeln!(f, "}}")?;
        }
        Ok(())
    }
}

fn write_stmt(f: &mut fmt::Formatter<'_>, stmt: &HakoStmt, indent: usize) -> fmt::Result {
    let ind = "  ".repeat(indent);
    match stmt {
        HakoStmt::Let { name, type_name, is_mut, value } => {
            write!(f, "{}let {}{}", ind, if *is_mut { "mut " } else { "" }, name)?;
            if let Some(ref t) = type_name { write!(f, ": {}", t)?; }
            if let Some(ref v) = value { write!(f, " = {}", v)?; }
            writeln!(f)?;
        }
        HakoStmt::Expr(e) => writeln!(f, "{}{};", ind, e)?,
        HakoStmt::Caso { cond, body } => {
            writeln!(f, "{}caso {} => {{", ind, cond)?;
            for s in body { write_stmt(f, s, indent + 1)?; }
            writeln!(f, "{}}}", ind)?;
        }
        HakoStmt::CasoContrario(body) => {
            writeln!(f, "{}caso contrário => {{", ind)?;
            for s in body { write_stmt(f, s, indent + 1)?; }
            writeln!(f, "{}}}", ind)?;
        }
        HakoStmt::Retorna(v) => {
            if let Some(ref v) = v { writeln!(f, "{}retorna {};", ind, v)?; }
            else { writeln!(f, "{}retorna;", ind)?; }
        }
        HakoStmt::Asm { template, operands, clobbers } => {
            write!(f, "{}asm!(\"{}\"", ind, template)?;
            if !clobbers.is_empty() {
                // check if last part of template is options
            }
            for op in operands {
                write!(f, ", {}", op)?;
            }
            for cl in clobbers {
                write!(f, ", {}", cl)?;
            }
            writeln!(f, ");")?;
        }
        HakoStmt::Loop(body) => {
            writeln!(f, "{}loop {{", ind)?;
            for s in body { write_stmt(f, s, indent + 1)?; }
            writeln!(f, "{}}}", ind)?;
        }
        HakoStmt::For { var, iter, body } => {
            writeln!(f, "{}for {} in {} {{", ind, var, iter)?;
            for s in body { write_stmt(f, s, indent + 1)?; }
            writeln!(f, "{}}}", ind)?;
        }
        HakoStmt::Block(body) => {
            writeln!(f, "{} {{", ind)?;
            for s in body { write_stmt(f, s, indent + 1)?; }
            writeln!(f, "{}}}", ind)?;
        }
        HakoStmt::Ref(r) => writeln!(f, "{}ref {};", ind, r)?,
        HakoStmt::Empty => {}
    }
    Ok(())
}
