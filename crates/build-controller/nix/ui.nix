{ pkgs }:
let
  a = {
    apiVersion = "apps/v1";
    kind = "Deployment";
    metadata.name = "job-list-ui";
    spec = {
      selector.matchLabels.app = "job-list-ui";
      template = {
        metadata.labels.app = "job-list-ui";
        spec.containers = [{
          name = "job-list-ui";
          image = "jobs-list-ui:I";
          ports = [{ containerPort = 3000; }];
        }];
      };
    };
  };
  b = {
    apiVersion = "apps/v1";
    kind = "Deployment";
    metadata.name = "job-ui";
    spec = {
      selector.matchLabels.app = "job-ui";
      template = {
        metadata.labels.app = "job-ui";
        spec.containers = [{
          name = "job-ui";
          image = "job-ui:I";
          ports = [{ containerPort = 3000; }];
        }];
      };
    };
  };
  c = {
    apiVersion = "v1";
    kind = "Service";
    metadata.name = "job-list-ui";
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
      namespace = "default";
    };
  };

  f = {
    apiVersion = "v1";
    kind = "ServiceAccount";
    metadata = {
      name = "jobs-list-ui";
      namespace = "default";
    };
  };

  g = {
    apiVersion = "rbac.authorization.k8s.io/v1";
    kind = "Role";
    metadata.name = "job-viewer";
    rules = [{
      apiGroups = [ "nixbuilds.build.example.com" ];
      resources = [ "jobs" ];
      verbs = [ "get" "list" "watch" ];
    }];
  };

  h = {

    apiVersion = "rbac.authorization.k8s.io/v1";
    kind = "RoleBinding";
    metadata.name = "job-list-ui-viewer";
    roleRef = {
      apiGroup = "rbac.authorization.k8s.io";
      kind = "Role";
      name = "job-viewer";
    };
    subjects = [{
      kind = "ServiceAccount";
      name = "job-list-ui";
    }];
  };

  i = {
    apiVersion = "rbac.authorization.k8s.io/v1";
    kind = "RoleBinding";
    metadata.name = "job-ui-viewer";
    roleRef = {
      apiGroup = "rbac.authorization.k8s.io";
      kind = "Role";
      name = "job-viewer";
    };
    subjects = [{
      kind = "ServiceAccount";
      name = "job-ui";
    }];
  };
  j = {
    apiVersion = "gateway.networking.k8s.io/v1";
    kind = "Gateway";
    metadata.name = "example-gateway";
    spec = {
      gatewayClassName = "example-gateway-class";

      listeners = [{
        name = "http";
        port = 80;
        protocol = "HTTP";
        allowedRoutes = { namespaces = { from = "Same"; }; };
      }];
    };
  };
  k = {
    apiVersion = "gateway.networking.k8s.io/v1";
    kind = "GatewayClass";
    metadata.name = "example-gateway-class";
    spec = { controllerName = "example.com/gateway-controller"; };
  };
  l = {
    apiVersion = "gateway.networking.k8s.io/v1";
    kind = "HTTPRoute";
    metadata.name = "job-list-ui";
    spec = {
      parentRefs = [{ name = "example-gateway"; }];
      rules = [{
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
      }];
    };
  };

  m = {
    apiVersion = "gateway.networking.k8s.io/v1";
    kind = "HTTPRoute";
    metadata.name = "job-ui";
    spec = {
      parentRefs = [{ name = "example-gateway"; }];
      rules = [{
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
      }];
    };
  };
in { resources = [ a b c d e f g h i j k l m ]; }
