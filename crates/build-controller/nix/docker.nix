{ pkgs, build-controller }:

let
  pname = "nix-build-controller";
  version = "0.1.0";

  image = pkgs.dockerTools.buildImage {
    name = pname;
    tag = version;
    config = {
      Cmd = [ "${build-controller}/bin/${pname}" ];
      Env = [ "RUST_LOG=info" ];
    };
  };

  manifests = pkgs.runCommand "build-controller-manifests" { } ''
    mkdir -p $out
    echo '${
      builtins.toJSON {
        apiVersion = "apps/v1";
        kind = "Deployment";
        metadata = {
          name = pname;
          namespace = "nixbuilder";
        };

        spec = {
          replicas = 1;
          selector.matchLabels = { app = pname; };
          template = {
            metadata.labels = { app = pname; };
            spec = {
              imagePullSecrets = [{ name = "nix-serve-regcred"; }];
              serviceAccountName = pname;
              containers = [{
                name = pname;
                image = "registry.fyfaen.as/nix-build-controller:1.0.37";
                env = [{
                  name = "RUST_LOG";
                  value = "info";
                }];
              }];
            };
          };
        };
      }
    }' > $out/deployment.yaml

    echo '${
      builtins.toJSON {
        apiVersion = "rbac.authorization.k8s.io/v1";
        kind = "Role";
        metadata = {
          name = pname;
          namespace = "nixbuilder";
        };
        rules = [
          {
            apiGroups = [ "batch" ];
            resources = [ "jobs" ];
            verbs = [ "create" "delete" "get" "list" "watch" "patch" ];
          }
          {
            apiGroups = [ "build.fyfaen.as" ];
            resources = [ "nixbuilds" ];
            verbs = [ "get" "list" "watch" "update" "patch" ];
          }
          {
            apiGroups = [ "build.fyfaen.as" ];
            resources = [ "nixbuilds/status" ];
            verbs = [ "patch" ];
          }
        ];
      }
    }' > $out/role.yaml

    echo '${
      builtins.toJSON {
        apiVersion = "rbac.authorization.k8s.io/v1";
        kind = "RoleBinding";
        metadata = {
          name = pname;
          namespace = "nixbuilder";
        };
        subjects = [{
          kind = "ServiceAccount";
          name = pname;
          namespace = "nixbuilder";
        }];
        roleRef = {
          kind = "Role";
          name = pname;
          apiGroup = "rbac.authorization.k8s.io";
        };
      }
    }' > $out/rolebinding.yaml

    echo '${
      builtins.toJSON {
        apiVersion = "v1";
        kind = "ServiceAccount";
        metadata = {
          name = pname;
          namespace = "nixbuilder";
        };
      }
    }' > $out/serviceaccount.yaml
  '';

in { inherit image manifests; }
