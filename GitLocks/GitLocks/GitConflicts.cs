using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading.Tasks;
using LibGit2Sharp;
using Optional;

namespace GitLocks
{
    public class GitConflicts
    {
        public class GitConflictException : Exception
        {
            public enum Reason
            {
                ConflictingCommitExistsInGlobalGraph,
                LocalBranchInvalid
            }

            public Reason reason;

            public GitConflictException(Reason reason, string message) : base(message)
            {
                this.reason = reason;
            }
        }

        public static Option<Unit, GitConflictException> PreCommit(string repoPath)
        {
            using (Repository localRepo = new Repository(repoPath))
            {
                // first make sure we're all pushed
                PushToGlobalGraph(localRepo);

                var files = localRepo.Index.Select(entry => entry.Path).ToArray();

                string globalRepoPath = localRepo.Config.Get<string>("locks.globalrepopath").Value;

                return RequestModifyFiles(globalRepoPath, MapLocalBranchNameToGlobal(localRepo, localRepo.Head), files);
            }
        }

        public static Option<Unit, GitConflictException> PostCommit(string repoPath)
        {
            using (Repository localRepo = new Repository(repoPath))
            {
                PushToGlobalGraph(localRepo);
            }

            return Unit.Default.Some<Unit, GitConflictException>();
        }

        private static void PushToGlobalGraph(Repository localRepo)
        {
            string[] refspecs = localRepo.Branches
                                         .Where(branch => !branch.IsRemote)
                                         .Select(branch =>
                                             $"{branch.CanonicalName}:{MapLocalBranchNameToGlobal(localRepo, branch)}")
                                         .ToArray();

            localRepo.Network.Push(localRepo.Network.Remotes["global"], refspecs);
        }

        // On Server
        private static Option<Unit, GitConflictException> RequestModifyFiles(
            string globalRepoPath, string currentBranch, string[] filePaths)
        {
            using (Repository globalRepo = new Repository(globalRepoPath))
            {
                // check each file path for modifications in mergeable branches

                foreach (Branch conflictingBranch in globalRepo.Branches)
                {
                    foreach (string filePath in filePaths)
                    {
                        var firstCommit =
                            conflictingBranch.Commits.FirstOrDefault(commit => commit.Tree.Any(entry => entry.Path == filePath));

                        if (firstCommit == null)
                        {
                            // No commits on this branch touch the file, so we're clear to commit
                            continue;
                        }

                        // Verify that the current branch descends from this commit, 
                        IEnumerable<Reference> descendantReferences =
                            globalRepo.Refs.ReachableFrom(new[] {firstCommit});

                        Branch globalBranchForLocal =
                            globalRepo.Branches.FirstOrDefault(branch => branch.CanonicalName == currentBranch);
                        if (globalBranchForLocal == null)
                        {
                            // If the local branch doesn't exist in the global graph, then the local branch is an empty branch (ie. points at nothing).
                            // If this is the case, it certainly does not descend from the conflicting commit.
                            return Option.None<Unit, GitConflictException>(
                                new GitConflictException(
                                    GitConflictException.Reason.LocalBranchInvalid,
                                    $"The commit [{firstCommit.Sha.Substring(0, 6)}] existing on branch [{conflictingBranch.CanonicalName}] " +
                                    $"would conflict with this commit. Current branch must incorporate that commit first."));
                        }

                        Reference currentBranchReference = globalBranchForLocal.Reference;

                        if (!descendantReferences.Contains(currentBranchReference))
                        {
                            // Our current branch is not a descendant of the modified commit.
                            return Option.None<Unit, GitConflictException>(
                                new GitConflictException(GitConflictException.Reason.ConflictingCommitExistsInGlobalGraph,
                                    $"The commit [{firstCommit.Sha.Substring(0, 6)}] existing on branch [{conflictingBranch.CanonicalName}] " +
                                    $"would conflict with this commit. Current branch must incorporate that commit first."));
                        }
                    }
                }
            }

            return Option.Some<Unit, GitConflictException>(Unit.Default);
        }

        public static string MapLocalBranchNameToGlobal(Repository localRepo, Branch localBranch)
        {
            // TODO: Handle missing repoid
            string localId = GetRepositoryGUID(localRepo);

            return $"refs/heads/{localId}/{localBranch.FriendlyName}";
        }

        public static string GetRepositoryGUID(Repository localRepo)
        {
            ConfigurationEntry<string> localId = localRepo.Config.Get<string>("locks.repositoryuuid");

            if (localId == null)
            {
                //TODO(john): UUID should map uniquely. Currently doesn't handle two users with the same name and hostname.
                string name = localRepo.Config.Get<string>("user.name").Value;
                Regex rgx = new Regex("[^a-zA-Z0-9]");
                string user_name_cleaned = rgx.Replace(name, "");

                byte[] guidBytes = Guid.NewGuid().ToByteArray();
                string guid = BitConverter.ToString(guidBytes).Replace("-", "").Substring(0, 10);

                string id = $"{user_name_cleaned}_{Environment.MachineName}_{guid}".ToLower();
                localRepo.Config.Set("locks.repositoryuuid", id);
                return id;
            }
            else
            {
                return localId.Value;
            }
        }
    }
}
