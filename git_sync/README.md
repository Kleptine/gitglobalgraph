Git Global Graph is a layer on top of Git that keeps track of all the work being done in every local clone of a repository. It then provides a system to make queries on this shared information: the Global Graph.

[[screenshot]]

The initial implementation of Git Global Graph is used to track and reject commits that modify binary files in parallel.

## Status
The project is currently suitable for small usage and limited production. It has not been tested on larger workflows.


