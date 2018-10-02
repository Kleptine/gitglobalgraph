using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading.Tasks;
using LibGit2Sharp;

namespace GitLocks
{
    public class Utils
    {
        /// <summary>
        /// Takes a local repository reference and a local branch and returns the name of the branch
        /// that matches it in the global repository.
        /// </summary>
        public static string MapLocalBranchNameToGlobal(Repository localRepo, Branch localBranch)
        {
            string localId = GetRepositoryGuid(localRepo);

            return MapLocalBranchNameToGlobal(localId, localBranch.FriendlyName);
        }

        /// <summary>
        /// Takes a local repository reference and a local branch and returns the name of the branch
        /// that matches it in the global repository.
        /// </summary>
        public static string MapLocalBranchNameToGlobal(string clientRepoGuid, string clientBranchNameFriendly)
        {
            return $"refs/heads/{clientRepoGuid}/{clientBranchNameFriendly}";
        }

        /// <summary>
        /// Gets a unique string identifier for a repository. This is build to be partially human readable, but unique to
        /// a degree of randomness.
        /// </summary>
        /// <param name="localRepo"></param>
        /// <returns></returns>
        public static string GetRepositoryGuid(Repository localRepo)
        {
            // Try to get the guid from settings if it exists. 
            ConfigurationEntry<string> localId = localRepo.Config.Get<string>("locks.repositoryguid");

            if (localId == null)
            {
                string name = localRepo.Config.Get<string>("user.name").Value;
                Regex rgx = new Regex("[^a-zA-Z0-9]");
                string userNameCleaned = rgx.Replace(name, "");

                byte[] guidBytes = Guid.NewGuid().ToByteArray();
                string guid = BitConverter.ToString(guidBytes).Replace("-", "").Substring(0, 10);

                string id = $"{userNameCleaned}_{Environment.MachineName}_{guid}".ToLower();

                // Set the guid in the local repository's settings.
                localRepo.Config.Set("locks.repositoryguid", id);

                return id;
            }

            return localId.Value;
        }
    }
}
