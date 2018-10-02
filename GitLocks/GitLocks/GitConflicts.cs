using System.Collections.Generic;
using System.Linq;
using LibGit2Sharp;
using NDepend.Path;
using Optional;

namespace GitLocks
{
    public partial class GitConflicts
    {
        public static readonly string GlobalConflictsServerRemoteName = "global-conflicts";

        /// <summary>
        /// Excecutes on the pre-commit Git hook. Verifies with the conflicts server that this commit is valid
        /// and won't conflict with any other already-committed changes.
        /// </summary>
        public static Option<Unit, GitConflictException> PreCommit(string repoPath)
        {
            using (Repository localRepo = new Repository(repoPath))
            {
                // first make sure we're all pushed
                SyncToGlobalGraph(localRepo);

                var files = localRepo.Index.Select(entry => entry.Path).ToArray();

                string globalRepoPath = localRepo.Config.Get<string>("locks.syncserverpath").Value;

                Server server = new Server(globalRepoPath.ToAbsoluteDirectoryPath());  
                return server.RequestModifyFiles(Utils.GetRepositoryGuid(localRepo), localRepo.Head.FriendlyName, files);
            }
        }

        /// <summary>
        /// Executes on the post-commit Git hook. Makes sure that the conflicts server is up to date on
        /// all of our new changes.
        /// </summary>
        public static Option<Unit, GitConflictException> PostCommit(string repoPath)
        {
            using (Repository localRepo = new Repository(repoPath))
            {
                SyncToGlobalGraph(localRepo);
            }

            return Unit.Default.Some<Unit, GitConflictException>();
        }

        /// <summary>
        /// Pushes the current repository state to the global conflicts server.
        /// </summary>
        private static void SyncToGlobalGraph(Repository localRepo)
        {
            if (!localRepo.Branches.Any())
            {
                return;
            }

            string[] refspecs = localRepo.Branches
                                         .Where(branch => !branch.IsRemote)
                                         .Select(branch =>
                                             $"{branch.CanonicalName}:{Utils.MapLocalBranchNameToGlobal(localRepo, branch)}")
                                         .ToArray();

            // For now, we can just push all of our references up to the server.
            localRepo.Network.Push(localRepo.Network.Remotes[GlobalConflictsServerRemoteName], refspecs);
        }

    }
}
