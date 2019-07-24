FROM gitpod/workspace-full:latest

RUN apt-get update && apt-get install -yq fuse libfuse-dev