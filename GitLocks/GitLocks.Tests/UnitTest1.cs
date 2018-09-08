using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Conditions;
using LibGit2Sharp;
using Microsoft.VisualStudio.TestTools.UnitTesting;
using NDepend.Path;
using Optional;

namespace GitLocks.Tests
{
    [TestClass]
    public class UnitTest1
    {
        [TestMethod]
        public void SimpleCommit()
        {
            RunRepoTest((localRepo, _, __, globalGraphRepo) =>
            {
                // Make some content changes in the local repo.
                IAbsoluteDirectoryPath localRepoPath = localRepo.Info.WorkingDirectory.ToAbsoluteDirectoryPath();
                IRelativeFilePath localFile = @".\file.bin".ToRelativeFilePath();
                File.WriteAllText(localRepoPath.GetChildFileWithName(localFile.FileName).ToString(), "test contents");

                localRepo.Index.Add(localFile.FileName);

                // Test the precommit hook.
                var result = GitConflicts.PreCommit(localRepo.Info.Path);
                Condition.Requires(result.HasValue).IsTrue();

                Signature author = new Signature("John", "@kleptine", DateTime.Now);
                Signature committer = author;
                localRepo.Commit("Local topic change.", author, committer);
                GitConflicts.PostCommit(localRepo.Info.Path);

                Condition.Requires(localRepo.Commits).HasLength(1);
                Condition.Requires(globalGraphRepo.Branches).HasLength(1);

                string globalBranchName = $"refs/heads/{GitConflicts.GetRepositoryGUID(localRepo)}/master";

                // Check the local branch pushed to the correct remote name
                Condition.Requires(globalGraphRepo.Branches.ElementAt(0).CanonicalName)
                         .IsEqualTo(globalBranchName);

                // Check the remote branch that was pushed has the correct commit sha's
                Condition.Requires(globalGraphRepo.Branches[globalBranchName].Commits.ElementAt(0).Sha)
                         .IsEqualTo(localRepo.Commits.ElementAt(0).Sha);
            });
        }

        [TestMethod]
        public void SimplestConflict()
        {
            RunRepoTest((localRepoA, localRepoB, __, globalGraphRepo) =>
            {
                // Modify file.bin in repo A.
                IAbsoluteDirectoryPath localRepoAPath = localRepoA.Info.WorkingDirectory.ToAbsoluteDirectoryPath();
                IRelativeFilePath localAFile = @".\file.bin".ToRelativeFilePath();
                File.WriteAllText(localRepoAPath.GetChildFileWithName(localAFile.FileName).ToString(), "test contents");
                localRepoA.Index.Add(localAFile.FileName);

                var result = GitConflicts.PreCommit(localRepoA.Info.Path);
                Condition.Requires(result.HasValue).IsTrue();

                Signature author = new Signature("John", "@kleptine", DateTime.Now);
                localRepoA.Commit("Change file.bin.", author, author);
                GitConflicts.PostCommit(localRepoA.Info.Path);

                // Modify file.bin in repo B.
                IAbsoluteDirectoryPath localRepoBPath = localRepoB.Info.WorkingDirectory.ToAbsoluteDirectoryPath();
                IRelativeFilePath localBFile = @".\file.bin".ToRelativeFilePath();
                File.WriteAllText(localRepoBPath.GetChildFileWithName(localBFile.FileName).ToString(), "test contents");
                localRepoB.Index.Add(localBFile.FileName);
                localRepoB.Index.Write();

                result = GitConflicts.PreCommit(localRepoB.Info.Path);
                Condition.Requires(result.HasValue).IsFalse();
                result.Match(
                    _ => Assert.Fail("Precommit operation should have failed"),
                    exception => Condition.Requires(exception.reason)
                                          .IsEqualTo(GitConflicts.GitConflictException.Reason.LocalBranchInvalid));

                // Abort commit, conflicting change! Local branch has no commits, so definitely doesn't descend from commit in repo A.
            });
        }

        [TestMethod]
        public void SimpleConflict()
        {
            RunRepoTest((localRepoA, localRepoB, __, globalGraphRepo) =>
            {
                // Modify file.bin in repo A.
                Condition.Requires(ModifyFile(localRepoA, "file.bin", "Change file.bin").HasValue).IsTrue();

                Condition.Requires(ModifyFile(localRepoB, "file.txt", "Change file.txt, (should work fine)").HasValue)
                         .IsTrue();

                // Modify file.bin -- should conflict.
                var result = ModifyFile(localRepoB, "file.bin", "Conflicting change");
                result.Match(
                    _ => Assert.Fail("Precommit operation should have failed"),
                    exception => Condition.Requires(exception.reason)
                                          .IsEqualTo(GitConflicts
                                                     .GitConflictException.Reason
                                                     .ConflictingCommitExistsInGlobalGraph));
                // Abort commit, conflicting change!
            });
        }

        [TestMethod]
        public void SimpleConflict2()
        {
            RunRepoTest((localRepoA, localRepoB, originRepo, globalGraphRepo) =>
            {
                // Modify file.bin in repo A.
                Condition.Requires(ModifyFile(localRepoA, "file.bin", "Change file.bin").HasValue).IsTrue();

                // Push changes to origin
                localRepoA.Network.Push(localRepoA.Network.Remotes["origin"], "refs/heads/master");

                // Pull on B
                Remote remote = localRepoB.Network.Remotes["origin"];
                IEnumerable<string> refSpecs = remote.FetchRefSpecs.Select(x => x.Specification);
                Commands.Fetch(localRepoB, remote.Name, refSpecs, null, "");
                CheckoutB(localRepoB, "origin", "master");

                // Make more changes on A.
                Condition.Requires(ModifyFile(localRepoA, "file.bin", "Second change to file.bin", "new contents").HasValue).IsTrue();

                // Modify file.bin -- should conflict.
                var result = ModifyFile(localRepoB, "file.bin", "Second change to file.bin", "contents 2");
                result.Match(
                    _ => Assert.Fail("Precommit operation should have failed"),
                    exception => Condition.Requires(exception.reason)
                                          .IsEqualTo(GitConflicts
                                                     .GitConflictException.Reason
                                                     .ConflictingCommitExistsInGlobalGraph));
            });
        }

        [TestMethod]
        public void IncorporatedChanges()
        {
            RunRepoTest((localRepoA, localRepoB, originRepo, globalGraphRepo) =>
            {
                // Modify file.bin in repo A.
                Condition.Requires(ModifyFile(localRepoA, "file.bin", "Change file.bin").HasValue).IsTrue();

                // Push changes to origin
                localRepoA.Network.Push(localRepoA.Network.Remotes["origin"], "refs/heads/master");

                // Pull on B
                Remote remote = localRepoB.Network.Remotes["origin"];
                IEnumerable<string> refSpecs = remote.FetchRefSpecs.Select(x => x.Specification);
                Commands.Fetch(localRepoB, remote.Name, refSpecs, null, "");

                CheckoutB(localRepoB, "origin", "master");

                // Modify file.bin -- should work without conflicts!
                Condition.Requires(ModifyFile(localRepoB, "file.bin", "Second change to file.bin", "contents 2")
                    .HasValue).IsTrue();
            });
        }

        private void CheckoutB(Repository repo, string remoteName, string branchName)
        {
            Branch trackedBranch = repo.Branches[$"{remoteName}/{branchName}"];
            Branch localBranch = repo.CreateBranch(branchName, trackedBranch.Tip);
            repo.Branches.Update(localBranch, b =>
            {
                b.UpstreamBranch = $"refs/heads/{branchName}";
                b.Remote = remoteName;
            });
            Commands.Checkout(repo, repo.Branches[branchName], new CheckoutOptions());
        }

        private Option<Unit, GitConflicts.GitConflictException> ModifyFile(
            Repository repo, string fileName, string commitMessage,
            string fileContents = "test contents")
        {
            Signature author = new Signature("John", "@kleptine", DateTime.Now);

            IAbsoluteDirectoryPath repoPath = repo.Info.WorkingDirectory.ToAbsoluteDirectoryPath();
            IRelativeFilePath localFile = $@".\{fileName}".ToRelativeFilePath();
            File.WriteAllText(repoPath.GetChildFileWithName(localFile.FileName).ToString(), fileContents);
            repo.Index.Add(localFile.FileName);
            repo.Index.Write();

            var result = GitConflicts.PreCommit(repo.Info.Path);
            if (!result.HasValue)
            {
                return result;
            }

            repo.Commit(commitMessage, author, author);
            return GitConflicts.PostCommit(repo.Info.Path);
        }

        private void RunRepoTest(Action<Repository, Repository, Repository, Repository> test)
        {
            using (ITempDir tempDir = TempDir.Create())
            {
                IAbsoluteDirectoryPath dir = tempDir.Path.ToAbsoluteDirectoryPath();

                IAbsoluteDirectoryPath localRepoAPath = dir.GetChildDirectoryWithName("repoA");
                IAbsoluteDirectoryPath localRepoBPath = dir.GetChildDirectoryWithName("repoB");
                IAbsoluteDirectoryPath originRepoPath = dir.GetChildDirectoryWithName("origin");
                IAbsoluteDirectoryPath globalGraphPath = dir.GetChildDirectoryWithName("global_graph");

                Repository.Init(localRepoAPath.ToString());
                Repository.Init(localRepoBPath.ToString());
                Repository.Init(originRepoPath.ToString(), true);
                Repository.Init(globalGraphPath.ToString(), true);

                using (Repository localRepoA = new Repository(localRepoAPath.ToString()))
                {
                    using (Repository localRepoB = new Repository(localRepoBPath.ToString()))
                    {
                        using (Repository originRepo = new Repository(originRepoPath.ToString()))
                        {
                            using (Repository globalGraphRepo = new Repository(globalGraphPath.ToString()))
                            {
                                // Setup local repository.
                                localRepoA.Config.Set("locks.globalrepopath", globalGraphPath.ToString());
                                localRepoA.Config.Set("user.name", "Test User A");
                                localRepoA.Network.Remotes.Add("global", globalGraphPath.ToString());
                                localRepoA.Network.Remotes.Add("origin", originRepoPath.ToString());

                                // Setup local repository.
                                localRepoB.Config.Set("locks.globalrepopath", globalGraphPath.ToString());
                                localRepoB.Config.Set("user.name", "Test User B");
                                localRepoB.Network.Remotes.Add("global", globalGraphPath.ToString());
                                localRepoB.Network.Remotes.Add("origin", originRepoPath.ToString());

                                // Run the test
                                test(localRepoA, localRepoB, originRepo, globalGraphRepo);
                            }
                        }
                    }
                }
            }
        }
    }
}
