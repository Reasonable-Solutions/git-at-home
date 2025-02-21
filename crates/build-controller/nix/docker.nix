{ pkgs, build-controller }:

# TOOD: all of these guys should have a labels saying where tehy come from
let
  pname = "nix-build-controller";
  version = "0.1.0";

  image = pkgs.dockertools.buildImage {
    name = pname;
    tag = version;
    config = {
      Cmd = [ "${build-controller}/bin/${pname}" ];
      Env = [ "RUST_LOG=info" ];
    };
  };

  manifests = pkgs.runCommand "build-controller-manifests" { } ''
    mkdir -p $out
    ${build-controller}/bin/make-crd > $out/crd.yaml

    echo '${
      builtins.toJSON {
        apiVersion = "apps/v1";
        kind = "Deployment";
        metadata = { name = pname; };
        spec = {
          replicas = 1;
          selector.matchLabels = { app = pname; };
          template = {
            metadata.labels = { app = pname; };
            spec = {
              serviceAccountName = pname;
              containers = [{
                name = pname;
                image = "${pname}:VIII";
                imagePullPolicy = "Never";
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
        kind = "ClusterRole";
        metadata = {
          name = pname;
          namespace = "default";
        };
        rules = [
          {
            apiGroups = [ "batch" ];
            resources = [ "jobs" ];
            verbs = [ "create" "delete" "get" "list" "watch" ];
          }
          {
            apiGroups = [ "build.example.com" ];
            # Subresources appears to need to be explicitly specified v0v
            resources = [ "nixbuilds" "nixbuilds/status" ];

            # TODO:  Create only exists here for creating nixbuilds/status, not nixbuilds.
            verbs = [ "get" "list" "watch" "update" "create" "patch" ];
          }
        ];
      }
    }' > $out/role.yaml

    echo '${
      builtins.toJSON {
        apiVersion = "rbac.authorization.k8s.io/v1";
        kind = "ClusterRoleBinding";
        metadata = { name = pname; };
        subjects = [{
          kind = "ServiceAccount";
          name = pname;
          namespace = "default";
        }];
        roleRef = {
          kind = "ClusterRole";
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
          namespace = "default";
        };
      }
    }' > $out/serviceaccount.yaml
  '';

in { inherit image manifests; }
