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
  # TODO: make-crd, the bin should be parametrized over pname too
  manifests = pkgs.runCommand "build-controller-manifests" { } ''
    mkdir -p $out
    ${build-controller}/bin/make-crd > $out/templates/crd.yaml

    echo '${
        builtins.toJson {
          image:
            repository: europe-north1-docker.pkg.dev/nais-io/nais/images/nix-build-controller
            pullPolicy: Always
            tag: latest

         clusterName: dev-nais-dev
         tenant: dev.nais.io 

         nixbuild:
           nixserveUrl: # Url for nix serve,

      }
    }' > $out/templates/values.yaml  
    echo '${
      builtins.toJSON {
        apiVersion = "apps/v1";
        kind = "Deployment";
        metadata = { name = pname; namespace = "nais-systems" };
        spec = {
          replicas = 1;
          selector.matchLabels = { app = pname; };
          template = {
            metadata.labels = { app = pname; };
            spec = {
              serviceAccountName = pname;
              containers = [{
                name = pname;
                image = "${pname}:I";
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
    }' > $out/templates/deployment.yaml

    echo '${
      builtins.toJSON {
        apiVersion = "rbac.authorization.k8s.io/v1";
        kind = "Role";
        metadata = {
          name = pname;
          namespace = "nais-system";
        };
        rules = [
          {
            apiGroups = [ "batch" ];
            resources = [ "jobs" ];
            verbs = [ "create" "delete" "get" "list" "watch" ];
          }
          {
            apiGroups = [ "build.nais.io" ];
            resources = [ "nixbuilds" "nixbuilds/status" ];
            verbs = [ "get" "list" "watch" "update" "create" "patch" ];
          }
        ];
      }
    }' > $out/templates/role.yaml

    echo '${
      builtins.toJSON {
        apiVersion = "rbac.authorization.k8s.io/v1";
        kind = "RoleBinding";
        metadata = { name = pname; namespace = "nais-system" };
        subjects = [{
          kind = "ServiceAccount";
          name = pname;
          namespace = "nais-system";
        }];
        roleRef = {
          kind = "ClusterRole";
          name = pname;
          apiGroup = "rbac.authorization.k8s.io";
        };
      }
    }' > $out/templates/rolebinding.yaml

    echo '${
      builtins.toJSON {
        apiVersion = "v1";
        kind = "ServiceAccount";
        metadata = {
          name = pname;
          namespace = "nais-systems";
        };
      }
    }' > $out/templates/serviceaccount.yaml
  '';

in { inherit image manifests; }
