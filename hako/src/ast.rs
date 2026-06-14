use std::fmt;

#[derive(Debug)]
pub struct Program {
    pub boxes: Vec<Box>,
    pub flows: Vec<Flow>,
}

#[derive(Debug)]
pub struct Box {
    pub name: String,
    pub extends: Option<String>,
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub enum Item {
    Var { name: String, value: Option<String> },
    Fn {
        name: String,
        params: Vec<(String, String)>,
        mode: FnMode,
        body: Vec<Stmt>,
    },
    Expr(String),
}

#[derive(Debug)]
pub enum FnMode {
    Auto,
    Block,
    Raw,
}

#[derive(Debug)]
pub enum Stmt {
    If {
        cond: String,
        then: Vec<Stmt>,
        r#else: Vec<Stmt>,
    },
    Loop(Vec<Stmt>),
    For { var: String, iter: String, body: Vec<Stmt> },
    Break,
    Assign { var: String, value: String },
    Expr(String),
    Block(Vec<Stmt>),
    AsmOut { reg: String, var: String },
    AsmIn { reg: String, var: String },
    AsmLine(String),
}

#[derive(Debug)]
pub struct Flow {
    pub name: String,
    pub steps: Vec<String>,
}

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in &self.boxes {
            write!(f, "box {}", b.name)?;
            if let Some(ref ext) = b.extends { write!(f, "::{}", ext)?; }
            writeln!(f, " {{")?;
            for item in &b.items { write_item(f, item, 1)?; }
            writeln!(f, "}}")?;
        }
        for fl in &self.flows {
            writeln!(f, "flow {} {{", fl.name)?;
            for step in &fl.steps { writeln!(f, "  {}", step)?; }
            writeln!(f, "}}")?;
        }
        Ok(())
    }
}

fn write_item(f: &mut fmt::Formatter<'_>, item: &Item, indent: usize) -> fmt::Result {
    let ind = "  ".repeat(indent);
    match item {
        Item::Var { name, value } => {
            write!(f, "{}{}", ind, name)?;
            if let Some(ref v) = value { write!(f, " = {}", v)?; }
            writeln!(f)?;
        }
        Item::Fn { name, params, mode, body } => {
            write!(f, "{}{}(", ind, name)?;
            for (i, (p, pt)) in params.iter().enumerate() {
                if i > 0 { write!(f, ", ")?; }
                write!(f, "{}", p)?;
                if !pt.is_empty() { write!(f, ": {}", pt)?; }
            }
            write!(f, ")")?;
            match mode {
                FnMode::Auto => writeln!(f, " => default")?,
                FnMode::Block => { writeln!(f, " {{")?; for s in body { write_stmt(f, s, indent + 1)?; } writeln!(f, "{}}}", ind)?; }
                FnMode::Raw => { writeln!(f, " raw {{")?; for s in body { write_stmt(f, s, indent + 1)?; } writeln!(f, "{}}}", ind)?; }
            }
        }
        Item::Expr(e) => writeln!(f, "{}{};", ind, e)?,
    }
    Ok(())
}

fn write_stmt(f: &mut fmt::Formatter<'_>, stmt: &Stmt, indent: usize) -> fmt::Result {
    let ind = "  ".repeat(indent);
    match stmt {
        Stmt::If { cond, then, r#else } => {
            writeln!(f, "{}if {} {{", ind, cond)?;
            for s in then { write_stmt(f, s, indent + 1)?; }
            if r#else.is_empty() { writeln!(f, "{}}}", ind)?; }
            else { writeln!(f, "{}}} else {{", ind)?; for s in r#else { write_stmt(f, s, indent + 1)?; } writeln!(f, "{}}}", ind)?; }
        }
        Stmt::Loop(body) => { writeln!(f, "{}loop {{", ind)?; for s in body { write_stmt(f, s, indent + 1)?; } writeln!(f, "{}}}", ind)?; }
        Stmt::For { var, iter, body } => { writeln!(f, "{}for {} in {} {{", ind, var, iter)?; for s in body { write_stmt(f, s, indent + 1)?; } writeln!(f, "{}}}", ind)?; }
        Stmt::Break => writeln!(f, "{}break", ind)?,
        Stmt::Assign { var, value } => writeln!(f, "{}{} = {}", ind, var, value)?,
        Stmt::Expr(e) => writeln!(f, "{}{}", ind, e)?,
        Stmt::Block(body) => { writeln!(f, "{} {{", ind)?; for s in body { write_stmt(f, s, indent + 1)?; } writeln!(f, "{}}}", ind)?; }
        Stmt::AsmOut { reg, var } => writeln!(f, "{}out({}, {})", ind, reg, var)?,
        Stmt::AsmIn { reg, var } => writeln!(f, "{}in({}, {})", ind, reg, var)?,
        Stmt::AsmLine(s) => writeln!(f, "{}\"{}\"", ind, s)?,
    }
    Ok(())
}
