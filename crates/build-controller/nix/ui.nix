{ pkgs }:
let
  a = {
    apiVersion = "apps/v1";
    kind = "Deployment";
    metadata.name = "job-list-ui";
    metadata.namespace = "nixbuilder";
    spec = {
      selector.matchLabels.app = "job-list-ui";
      template = {
        metadata.labels.app = "job-list-ui";
        spec = {
          serviceAccountName = "job-list-ui";
          imagePullSecrets = [{ name = "nix-serve-regcred"; }];
          containers = [{
            name = "job-list-ui";
            image = "registry.fyfaen.as/nix-jobs-list-ui:1.0.1";
            ports = [{ containerPort = 3000; }];
          }];
        };
      };
    };
  };

  b = {
    apiVersion = "apps/v1";
    kind = "Deployment";
    metadata.name = "job-ui";
    metadata.namespace = "nixbuilder";
    spec = {
      selector.matchLabels.app = "job-ui";
      template = {
        metadata.labels.app = "job-ui";
        spec = {
          serviceAccountName = "job-ui";
          imagePullSecrets = [{ name = "nix-serve-regcred"; }];
          containers = [{
            name = "jobs-ui";
            image = "registry.fyfaen.as/nix-jobs-ui:1.0.0";
            ports = [{ containerPort = 3000; }];
          }];
        };

      };
    };
  };

  c = {
    apiVersion = "v1";
    kind = "Service";
    metadata.name = "job-list-ui";
    metadata.namespace = "nixbuilder";
    spec = {
      ports = [{
        port = 80;
        targetPort = 3000;
      }];
      selector.app = "job-list-ui";
    };
  };
  d = {
    apiVersion = "v1";
    kind = "Service";
    metadata.name = "job-ui";
    metadata.namespace = "nixbuilder";
    spec = {
      ports = [{
        port = 80;
        targetPort = 3000;
      }];
      selector.app = "job-ui";
    };
  };
  e = {
    apiVersion = "v1";
    kind = "ServiceAccount";
    metadata = {
      name = "job-ui";
      namespace = "nixbuilder";
    };
  };

  f = {
    apiVersion = "v1";
    kind = "ServiceAccount";
    metadata = {
      name = "job-list-ui";
      namespace = "nixbuilder";
    };
  };

  g = {
    apiVersion = "rbac.authorization.k8s.io/v1";
    kind = "Role";
    metadata.name = "job-viewer";
    metadata.namespace = "nixbuilder";
    rules = [{
      apiGroups = [ "build.fyfaen.as" ];
      resources = [ "nixbuilds" ];
      verbs = [ "get" "list" "watch" ];
    }];
  };

  h = {

    apiVersion = "rbac.authorization.k8s.io/v1";
    kind = "RoleBinding";
    metadata.name = "job-list-ui-viewer";
    metadata.namespace = "nixbuilder";
    roleRef = {
      apiGroup = "rbac.authorization.k8s.io";
      kind = "Role";
      name = "job-viewer";
    };
    subjects = [{
      kind = "ServiceAccount";
      name = "job-list-ui";
      namespace = "nixbuilder";
    }];
  };

  i = {
    apiVersion = "rbac.authorization.k8s.io/v1";
    kind = "RoleBinding";
    metadata.name = "job-ui-viewer";
    metadata.namespace = "nixbuilder";
    roleRef = {
      apiGroup = "rbac.authorization.k8s.io";
      kind = "Role";
      name = "job-viewer";
    };
    subjects = [{
      kind = "ServiceAccount";
      name = "job-ui";
      namespace = "nixbuilder";
    }];
  };
  l = {
    apiVersion = "gateway.networking.k8s.io/v1";
    kind = "HTTPRoute";
    metadata = {
      name = "nix-ui";
      namespace = "nixbuilder";
    };
    spec = {
      hostnames = [ "nix.fyfaen.as" ];
      parentRefs = [{
        name = "cluster-gw";
        namespace = "nginx-gateway";
      }];
      rules = [
        {
          matches = [{
            path = {
              type = "PathPrefix";
              value = "/jobs";
            };
          }];
          backendRefs = [{
            name = "job-ui";
            port = 80;
          }];
        }
        {
          matches = [{
            path = {
              type = "PathPrefix";
              value = "/";
            };
          }];
          backendRefs = [{
            name = "job-list-ui";
            port = 80;
          }];
        }
        {

          matches = [{
            method = "DELETE";
            path = {
              type = "RegularExpression";
              value = "^/jobs-list/[^/]+$";
            };
          }];
          backendRefs = [{
            name = "job-list-ui";
            port = 80;
          }];
        }
      ];
    };
  };

in { resources = [ a b c d e f g h i l ]; }
