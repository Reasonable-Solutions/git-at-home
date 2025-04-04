use kube::CustomResourceExt;
use serde_yaml;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap_or_else(|_| "./crds".into());
    let path = Path::new(&out_dir);

    fs::create_dir_all(path).expect("create crds dir");

    let user = repo_controller::GitUser::crd();
    let repo = repo_controller::GitRepository::crd();
    let access = repo_controller::GitAccess::crd();

    fs::write(
        path.join("gituser.yaml"),
        serde_yaml::to_string(&user).unwrap(),
    )
    .unwrap();
    fs::write(
        path.join("gitrepository.yaml"),
        serde_yaml::to_string(&repo).unwrap(),
    )
    .unwrap();
    fs::write(
        path.join("gitaccess.yaml"),
        serde_yaml::to_string(&access).unwrap(),
    )
    .unwrap();
}
