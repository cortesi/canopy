use std::{env, fs};

use canopy::{backend::crossterm::runloop, *};
use canopy_examples::pager::{setup_bindings, Pager};

pub fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: pager filename");
    } else {
        let mut cnpy = Canopy::new();
        Root::<Pager>::load(&mut cnpy);
        setup_bindings(&mut cnpy);

        let contents = fs::read_to_string(args[1].clone())?;
        let root = Root::new(Pager::new(contents));
        runloop(cnpy, root)?;
    }
    Ok(())
}
