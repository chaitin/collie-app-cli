use std::error::Error;

use vergen::EmitBuilder;

fn main() -> Result<(), Box<dyn Error>> {
    // Emit the instructions
    EmitBuilder::builder()
        .all_git()
        .git_sha(true)
        .all_build()
        .all_cargo()
        .all_rustc()
        .emit()?;

    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=./windows/*");
        embed_resource::compile("./windows/app.rc", embed_resource::NONE);
    }
    Ok(())
}
