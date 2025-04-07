{ pkgs }:

pkgs.writeText "deployer-trivial-k8s.yaml" ''
  apiVersion: v1
  kind: ServiceAccount
  metadata:
    name: nix-deployer
    namespace: trivial
  ---
  apiVersion: rbac.authorization.k8s.io/v1
  kind: Role
  metadata:
    name: nix-deployer
    namespace: trivial
  rules:
    - apiGroups: ["apps"]
      resources: ["deployments"]
      verbs: ["get", "list", "watch", "create", "update", "patch"]
    - apiGroups: [""]
      resources: ["services"]
      verbs: ["get", "list", "watch", "create", "update", "patch"]
    - apiGroups: ["gateway.networking.k8s.io"]
      resources: ["httproutes"]
      verbs: ["get", "list", "watch", "create", "update", "patch"]
  ---
  apiVersion: rbac.authorization.k8s.io/v1
  kind: RoleBinding
  metadata:
    name: nix-deployer-binding
    namespace: trivial
  subjects:
    - kind: ServiceAccount
      name: nix-deployer
      namespace: trivial
  roleRef:
    kind: Role
    name: nix-deployer
    apiGroup: rbac.authorization.k8s.io
  ---
  apiVersion: apps/v1
  kind: Deployment
  metadata:
    name: nix-deployer
    namespace: trivial
  spec:
    replicas: 1
    selector:
      matchLabels:
        app: nix-deployer
    template:
      metadata:
        labels:
          app: nix-deployer
      spec:
        serviceAccountName: nix-deployer
        imagePullSecrets:
          - name: nix-serve-regcred
        securityContext:
          runAsUser: 1069
          runAsGroup: 1069
          fsGroup: 1069
        containers:
          - name: nix-deployer
            image: registry.fyfaen.as/nix-deploy:1.0.0
            command: ["/app/deploy"]
''
