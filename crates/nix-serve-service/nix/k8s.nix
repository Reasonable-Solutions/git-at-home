{ pkgs ? import <nixpkgs> { } }:
let
  manifests = pkgs.writeText "k8s.yaml" ''
    apiVersion: v1
    kind: PersistentVolumeClaim
    metadata:
      name: nix-serve-data-pvc
      namespace: nixbuilder
    spec:
      accessModes:
        - ReadWriteOnce
      resources:
        requests:
          storage: 20Gi
    ---
    apiVersion: apps/v1
    kind: Deployment
    metadata:
      name: nix-serve
      namespace: nixbuilder
    spec:
      replicas: 1
      selector:
        matchLabels:
          app: nix-serve
      template:
        metadata:
          labels:
            app: nix-serve
        spec:
          securityContext:
            runAsUser: 1069
            runAsGroup: 1069
            fsGroup: 1069
          imagePullSecrets:
            - name: "nix-serve-regcred"
          containers:
          - name: nix-serve
            image: registry.fyfaen.as/nix-serve-service:1.0.1
            ports:
            - containerPort: 3000
            volumeMounts:
            - name: nix-serve-data
              mountPath: /app/nar
          volumes:
          - name: nix-serve-data
            persistentVolumeClaim:
              claimName: nix-serve-data-pvc
    ---
    apiVersion: v1
    kind: Service
    metadata:
      name: nix-serve
      namespace: nixbuilder
    spec:
      selector:
        app: nix-serve
      ports:
        - protocol: TCP
          port: 3000
          targetPort: 3000
  '';
in { inherit manifests; }
