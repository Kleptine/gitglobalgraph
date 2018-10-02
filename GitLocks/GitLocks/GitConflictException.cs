using System;

namespace GitLocks
{
        public class GitConflictException : Exception
        {
            public enum ExceptionReason
            {
                /// <summary>
                /// Thrown when a second commit exists in the global graph that conflicts with
                /// the commit we are attempting to make, and we don't descend from it.
                /// </summary>
                ConflictingCommitInGlobalGraph,

                /// <summary>
                /// Thrown when a second commit exists in the global graph that conflicts with the commit we are attempting to make,
                /// and our local branch points at nothing. If our local branch is an empty branch, it definitely does not descend from
                /// the conflicting commit.
                /// </summary>
                ConflictingCommitInGlobalGraph_BranchInvalid
            }

            public readonly ExceptionReason Reason;

            public GitConflictException(ExceptionReason reason, string message) : base(message)
            {
                Reason = reason;
            }
        }
}
