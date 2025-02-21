{ pkgs ? import <nixpkgs> { } }:

let
  dockerImage = pkgs.dockerTools.buildImage {
    name = "nix-serve";
    tag = "latest";
    contents = with pkgs; [ nix-serve nix cacert ];

    config = {
      Cmd = [ "${pkgs.nix-serve}/bin/nix-serve" ];
      ExposedPorts = { "5000/tcp" = { }; };
    };
  };

  k8sYaml = pkgs.writeText "k8s.yaml" ''
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
            image: nix-serve:latest
            ports:
            - containerPort: 5000
            volumeMounts:
            - name: nix-store
              mountPath: /nix/store
          volumes:
          - name: nix-store
            persistentVolumeClaim:
              claimName: nix-store-pvc
  '';
in { inherit dockerImage k8sYaml; }
