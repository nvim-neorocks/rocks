use eyre::Result;
use lux_lib::project::Project;

pub fn debug_project() -> Result<()> {
    let project = Project::current()?;

    if let Some(project) = project {
        let rocks = project.toml();

        println!("Project Name: {}", rocks.package());
        println!("Project Version: {}", rocks.version());

        println!("Project location: {}", project.root().display());
    } else {
        eprintln!("Could not find project in current directory.");
    }

    Ok(())
}
