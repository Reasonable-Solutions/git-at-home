use build_controller::NixBuild;
use kube::CustomResourceExt;

fn main() {
    let crd = NixBuild::crd();
    println!("{}", serde_yaml::to_string(&crd).unwrap());
}
