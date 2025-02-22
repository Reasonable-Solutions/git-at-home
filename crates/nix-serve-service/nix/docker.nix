{ pkgs ? import <nixpkgs> { } }:
let
  manifests = pkgs.writeText "k8s.yaml" ''
    apiVersion: v1
    kind: PersistentVolumeClaim
    metadata:
      name: nix-serve-data-pvc
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
          containers:
          - name: nix-serve
            image: nix-serve-service:I
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
    spec:
      selector:
        app: nix-serve
      ports:
        - protocol: TCP
          port: 3000
          targetPort: 3000
  '';
in { inherit manifests; }
