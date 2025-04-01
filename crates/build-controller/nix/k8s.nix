{ pkgs, build-controller }:

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

  nixServePvc = {
    apiVersion = "v1";
    kind = "PersistentVolumeClaim";
    metadata = { name = "nix-serve-data-pvc"; };
    spec = {
      accessModes = [ "ReadWriteOnce" ];
      resources = { requests = { storage = "20Gi"; }; };
    };
  };

  nixServeDeployment = {
    apiVersion = "apps/v1";
    kind = "Deployment";
    metadata = { name = "nix-serve"; };
    spec = {
      replicas = 1;
      selector = { matchLabels = { app = "nix-serve"; }; };
      template = {
        metadata = { labels = { app = "nix-serve"; }; };
        spec = {
          containers = [{
            name = "nix-serve";
            image = "nix-serve-service";
            ports = [{ containerPort = 3000; }];
            volumeMounts = [{
              name = "nix-serve-data";
              mountPath = "/app/nar";
            }];
          }];
          volumes = [{
            name = "nix-serve-data";
            persistentVolumeClaim = { claimName = "nix-serve-data-pvc"; };
          }];
        };
      };
    };
  };

  nixServeService = {
    apiVersion = "v1";
    kind = "Service";
    metadata = { name = "nix-serve"; };
    spec = {
      selector = { app = "nix-serve"; };
      ports = [{
        protocol = "TCP";
        port = 3000;
        targetPort = 3000;
      }];
    };
  };

  # TODO: make-crd, the bin should be parametrized over pname too
  nixBuildControllerChart = pkgs.runCommand "build-controller-manifests" { } ''
      mkdir -p $out/templates
      ${build-controller}/bin/make-crd > $out/templates/crd.yaml

    ${
      let
        createK8sResource = resource: builtins.toJSON resource;

        resources = map createK8sResource [
          nixServeDeployment
          nixServeService
          nixServePvc
        ];
      in builtins.concatStringsSep ''

        ---
      '' resources
    } > $out/nix-serve-resources.yaml

      echo '${
        builtins.toJSON {
          apiVersion = "v2";
          name = "nix-build";
          description = "in-cluster nix builds";
          sources = [ "https://github.com/nais/nix-build" ];
          # A chart can be either an 'application' or a 'library' chart.
          #
          # Application charts are a collection of templates that can be packaged into versioned archives
          # to be deployed.
          #
          # Library charts provide useful utilities or functions for the chart developer. They're included as
          # a dependency of application charts to inject those utilities and functions into the rendering
          # pipeline. Library charts do not define any templates and therefore cannot be deployed.
          type = "application";

          # This is the chart version. This version number should be incremented each time you make changes
          # to the chart and its templates, including the app version.
          # Versions are expected to follow Semantic Versioning (https = //semver.org/)
          # The version is set by the Github workflow before packaging
          version = "invalid";
        }
      }' > $out/Chart.yaml

      echo '${
        builtins.toJSON {
          image = {
            repository =
              "europe-north1-docker.pkg.dev/nais-io/nais/images/nix-build-controller";
            pullPolicy = "Always";
            tag = "latest";
          };
          clusterName = "dev-nais-dev";
          tenant = "dev.nais.io";

          nixbuild.nixserveUrl = "nix-serve.svc";
        }
      }' > $out/values.yaml

      echo '${
        builtins.toJSON {
          apiVersion = "apps/v1";
          kind = "Deployment";
          metadata = {
            name = pname;
            namespace = "nais-systems";
          };
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
      }' > $out/templates/${pname}-deployment.yaml

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
      }' > $out/templates/${pname}-role.yaml

      echo '${
        builtins.toJSON {
          apiVersion = "rbac.authorization.k8s.io/v1";
          kind = "RoleBinding";
          metadata = {
            name = pname;
            namespace = "nais-system";
          };
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
      }' > $out/templates/${pname}-rolebinding.yaml

      echo '${
        builtins.toJSON {
          apiVersion = "v1";
          kind = "ServiceAccount";
          metadata = {
            name = pname;
            namespace = "nais-systems";
          };
        }
      }' > $out/templates/${pname}-serviceaccount.yaml
  '';

in { inherit image nixBuildControllerChart; }

