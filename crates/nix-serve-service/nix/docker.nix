{ pkgs ? import <nixpkgs> { } }:

let
  image = pkgs.dockerTools.buildImage {
    name = "nix-serve";
    tag = "latest";
    contents = with pkgs; [ nix-serve nix cacert ];

    config = {
      Cmd = [ "${pkgs.nix-serve}/bin/nix-serve" ];
      ExposedPorts = { "5000/tcp" = { }; };
    };
  };

  manifests = pkgs.writeText "k8s.yaml" ''
    apiVersion: v1
    kind: PersistentVolumeClaim
    metadata:
      name: nix-store-pvc
    spec:
      accessModes:
        - ReadWriteOnce
      resources:
        requests:
          storage: 50Gi
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
            image: nix-serve-service:VII
            imagePullPolicy: Never
            ports:
            - containerPort: 5000
            volumeMounts:
            - name: nix-store
              mountPath: /var/cache
          volumes:
          - name: nix-store
            persistentVolumeClaim:
              claimName: nix-store-pvc
  '';
in { inherit image manifests; }
