use eyre::Result;
use rocks_lib::project::Project;

pub fn debug_project() -> Result<()> {
    let project = Project::current()?;

    if let Some(project) = project {
        let rockspec = project.rockspec();

        println!("Project Name: {}", rockspec.package);
        println!("Project Version: {}", rockspec.version);

        println!("Project location: {}", project.root().display());
    } else {
        eprintln!("Could not find project in current directory.");
    }

    Ok(())
}
