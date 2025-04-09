{ pkgs ? import <nixpkgs> { } }:

pkgs.writeText "deployer-nixbuilder-k8s.yaml" ''
  apiVersion: v1
  kind: ServiceAccount
  metadata:
    name: nix-deployer
    namespace: nixbuilder
  ---
  apiVersion: rbac.authorization.k8s.io/v1
  kind: ClusterRole
  metadata:
    name: nix-deployer-global
  rules:
    - apiGroups: ["apps"]
      resources: ["deployments"]
      verbs: ["get", "list", "watch", "patch", "create"]
    - apiGroups: [""]
      resources: ["services"]
      verbs: ["get", "list", "watch", "patch", "create"]
    - apiGroups: ["gateway.networking.k8s.io"]
      resources: ["httproutes"]
      verbs: ["get", "list", "watch", "patch", "create"]
  ---
  apiVersion: rbac.authorization.k8s.io/v1
  kind: ClusterRoleBinding
  metadata:
    name: nix-deployer-global-binding
  subjects:
    - kind: ServiceAccount
      name: nix-deployer
      namespace: nixbuilder
  roleRef:
    kind: ClusterRole
    name: nix-deployer-global
    apiGroup: rbac.authorization.k8s.io
  ---
  apiVersion: apps/v1
  kind: Deployment
  metadata:
    name: nix-deployer
    namespace: nixbuilder
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
        containers:
          - name: nix-deployer
            image: registry.fyfaen.as/nix-deploy:1.0.4
            command: ["./deploy"]
''
