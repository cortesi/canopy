use std::{env, fs};

use canopy::{backend::crossterm::runloop, *};
use canopy_examples::cedit::{setup_bindings, Ed};

pub fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: cedit filename");
    } else {
        let mut cnpy = Canopy::new();
        Root::<Ed>::load(&mut cnpy);
        setup_bindings(&mut cnpy);

        let contents = fs::read_to_string(args[1].clone())?;
        runloop(cnpy, Root::new(Ed::new(contents)).with_inspector(false))?;
    }
    Ok(())
}
