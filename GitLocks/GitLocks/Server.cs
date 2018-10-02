using System;
using System.Collections.Generic;
using System.Data.SQLite;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using LibGit2Sharp;
using NDepend.Path;
using Optional;
using static GitLocks.GitConflictException.ExceptionReason;

namespace GitLocks
{
    public class Server
    {
        private IAbsoluteDirectoryPath workingDirectory;

        private string GlobalRepoPath => workingDirectory.GetChildDirectoryWithName("repository").ToString();

        public Server(IAbsoluteDirectoryPath workingDirectory)
        {
            this.workingDirectory = workingDirectory;
        }

        /// <summary>
        /// Marks the specific commit as a divergent commit. See documentation for what that means.
        /// </summary>
        /// <summary>
        /// Makes a request to modify files on a local client. 
        /// </summary>
        /// <param name="clientRepoGuid">The guid of the client repository that is trying to commit.</param>
        /// <param name="clientBranchFriendly">The local branch name the client is trying to commit to.</param>
        /// <param name="filePaths">A list of file paths in the client would like to commit.</param>
        /// <returns>An option with Unit if the request can proceed, otherwise an exception and reason for failure.</returns>
        public Option<Unit, GitConflictException> RequestModifyFiles(string clientRepoGuid,
                                                                      string clientBranchFriendly,
                                                                      string[] filePaths)
        {
            using (Repository globalRepo = new Repository(GlobalRepoPath))
            {
                // check each file path for modifications in mergeable branches

                foreach (Branch conflictingBranch in globalRepo.Branches)
                {
                    foreach (string filePath in filePaths)
                    {
                        // Find the first commit on the candidate branch that touches the file path.
                        var firstCommit =
                            conflictingBranch.Commits.FirstOrDefault(commit =>
                                commit.Tree.Any(entry => entry.Path == filePath));

                        if (firstCommit == null)
                        {
                            // No commits on this branch touch the file, so we're clear to commit
                            continue;
                        }

                        // Verify that the current branch descends from this commit, 
                        IEnumerable<Reference> descendantReferences =
                            globalRepo.Refs.ReachableFrom(new[] {firstCommit});

                        string globalBranchName =
                            Utils.MapLocalBranchNameToGlobal(clientRepoGuid, clientBranchFriendly);

                        Branch globalBranchForLocal =
                            globalRepo.Branches.FirstOrDefault(branch => branch.CanonicalName == globalBranchName);

                        if (globalBranchForLocal == null)
                        {
                            // If the local branch doesn't exist in the global graph, then the local branch is an empty branch (ie. points at nothing).
                            // If this is the case, it certainly does not descend from the conflicting commit.
                            return Option.None<Unit, GitConflictException>(
                                new GitConflictException(ConflictingCommitInGlobalGraph_BranchInvalid,
                                    $"The commit [{firstCommit.Sha.Substring(0, 6)}] existing on branch [{conflictingBranch.CanonicalName}] " +
                                    $"would conflict with this commit. Current branch must incorporate that commit first."));
                        }

                        Reference currentBranchReference = globalBranchForLocal.Reference;

                        if (!descendantReferences.Contains(currentBranchReference))
                        {
                            // Our current branch is not a descendant of the modified commit.
                            return Option.None<Unit, GitConflictException>(
                                new GitConflictException(ConflictingCommitInGlobalGraph,
                                    $"The commit [{firstCommit.Sha.Substring(0, 6)}] existing on branch [{conflictingBranch.CanonicalName}] " +
                                    $"would conflict with this commit. Current branch must incorporate that commit first."));
                        }
                    }
                }
            }

            return Option.Some<Unit, GitConflictException>(Unit.Default);
        }

        /// <summary>
        /// Returns all of the branches that conflict with the given branch.
        /// </summary>
        /// <param name="branch"></param>
        /// <returns></returns>
        private static List<Branch> GetConflictingBranches(Repository globalRepo, Branch branch)
        {
            // TODO(john): Add divergent checks.
            return globalRepo.Branches.ToList();
        }
    }
}
