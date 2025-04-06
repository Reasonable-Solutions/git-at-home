{ pkgs }:
let
  webhook = {
    deployment = {
      apiVersion = "apps/v1";
      kind = "Deployment";
      metadata = {
        name = "webhook";
        namespace = "nixbuilder";
      };
      spec = {
        selector.matchLabels.app = "webhook";
        template = {
          metadata.labels.app = "webhook";
          spec = {
            serviceAccountName = "webhook";
            imagePullSecrets = [{ name = "nix-serve-regcred"; }];
            containers = [{
              name = "webhook";
              image = "registry.fyfaen.as/nix-webhook:1.0.1";
              ports = [{ containerPort = 3000; }];
              env = [{
                # This should be a projected mount!
                name = "WEBHOOK_SECRET";
                valueFrom.secretKeyRef = {
                  name = "webhook-secret";
                  key = "token";
                };
              }];
            }];
          };
        };
      };
    };

    service = {
      apiVersion = "v1";
      kind = "Service";
      metadata = {
        name = "webhook";
        namespace = "nixbuilder";
      };
      spec = {
        ports = [{
          port = 80;
          targetPort = 3000;
        }];
        selector.app = "webhook";
      };
    };

    serviceAccount = {
      apiVersion = "v1";
      kind = "ServiceAccount";
      metadata = {
        name = "webhook";
        namespace = "nixbuilder";
      };
    };

    role = {
      apiVersion = "rbac.authorization.k8s.io/v1";
      kind = "Role";
      metadata = {
        name = "webhook-writer";
        namespace = "nixbuilder";
      };
      rules = [{
        apiGroups = [ "build.fyfaen.as" ];
        resources = [ "nixbuilds" ];
        verbs = [ "create" ];
      }];
    };

    roleBinding = {
      apiVersion = "rbac.authorization.k8s.io/v1";
      kind = "RoleBinding";
      metadata = {
        name = "webhook-writer-binding";
        namespace = "nixbuilder";
      };
      roleRef = {
        apiGroup = "rbac.authorization.k8s.io";
        kind = "Role";
        name = "webhook-writer";
      };
      subjects = [{
        kind = "ServiceAccount";
        name = "webhook";
        namespace = "nixbuilder";
      }];
    };

    httpRoute = {
      apiVersion = "gateway.networking.k8s.io/v1";
      kind = "HTTPRoute";
      metadata = {
        name = "webhook";
        namespace = "nixbuilder";
      };
      spec = {
        hostnames = [ "nix.fyfaen.as" ];
        parentRefs = [{
          name = "cluster-gw";
          namespace = "nginx-gateway";
        }];
        rules = [{
          matches = [{
            path = {
              type = "PathPrefix";
              value = "/trigger-build";
            };
          }];
          backendRefs = [{
            name = "webhook";
            port = 80;
          }];
        }];
      };
    };
  };
in webhook
