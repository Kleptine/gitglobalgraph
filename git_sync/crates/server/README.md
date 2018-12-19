The server component for the global graph. This component starts two things:
1. A **Git repository server**. This is the global graph repository.
2. The **Query Server** (HTTP). This is a server that can perform complex queries on top of the global graph and return the results to clients.

The GG Query Server can perform arbitrary tasks and currently supports the following queries:
 - **Find Conflicts**: Given a list of files and a current head, determine whether there are any commits in the global graph that would conflict with a new commit on the current head.
