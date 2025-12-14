use {
    std::{
        collections::HashSet,
        io::{self, Write, stdin, stdout},
    },
    two_phase_interner::Interner,
};

pub fn main() -> io::Result<()> {
    let mut interner: Interner<str> = Interner::new();

    let mut words = HashSet::new();
    let mut line = String::new();
    loop {
        line.clear();
        print!("> ");
        stdout().flush()?;

        stdin().read_line(&mut line)?;
        let linet = line.trim();
        if linet.is_empty() {
            break;
        }
        let atom = interner.intern(linet);
        if words.contains(&atom) {
            println!("String '{linet}' already interned as {atom:?}");
        } else {
            words.insert(atom);
            println!("'{linet}' = {atom:?}");
        }
    }

    interner.optimize();

    println!("\n== Interned atoms ==");
    for (atom, count) in interner.atoms() {
        let s = interner.resolve(atom).unwrap();
        println!("{atom:?} = '{s}' (count {count})");
    }

    Ok(())
}
