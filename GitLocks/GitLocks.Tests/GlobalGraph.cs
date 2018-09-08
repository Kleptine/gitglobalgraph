using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using LibGit2Sharp;

namespace GitLocks.Tests
{
    public class GlobalGraph
    {
        private Repository repo;

        public GlobalGraph(Repository repo)
        {
            this.repo = repo;
        }

        private IEnumerable<Branch> GetConflictingBranches(Branch toCommit)
        {
            return repo.Branches;
        }

        public void RequestModifyFiles(Branch headBranch, string[] changedFiles)
        {
            var potentialConflicts = GetConflictingBranches(headBranch);

            foreach (string file in changedFiles)
            {
                foreach (Branch potentialConflict in potentialConflicts)
                {
                    Commit mostRecentChangeToFile = potentialConflict.Commits.AsQueryable()
                                     .FirstOrDefault(commit => commit.Tree.Any(entry => entry.Path.Equals(file)));

                    if (mostRecentChangeToFile == null)
                    {
                        // Branch has no changes to file, so no conflicts.

                        continue;
                    }
                }
            }
        }
    }
}
