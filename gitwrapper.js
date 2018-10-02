function Repo(test_name) {


  var myTemplateConfig = {
    colors: ['#66c2a5', '#fc8d62', '#8da0cb', '#e78ac3', '#a6d854', '#ffd92f', '#e5c494'], // branches colors, 1 per column
    arrow: {
      size: 15,
      // offset: -1,
      color: "#000",
    },
    branch: {
      lineWidth: 4,
      color: "#000",
      mergeStyle: "straight",
      spacingX: 50,
      showLabel: true,                  // display branch names on graph
      labelFont: "normal 13pt Arial"
    },
    commit: {
      spacingY: -60,
      dot: {
        size: 12,
        strokeWidth: 8,
        strokeColor: "#000"
      },
      message: {
        displayAuthor: false,
        displayBranch: false,
        displayHash: true,
        color: "black"
      },
    }
  };
  var myTemplate = new GitGraph.Template(myTemplateConfig);


  this.gitgraph = new GitGraph({
    template: myTemplate,
    reverseArrow: false,
    orientation: "vertical-reverse",
    elementId: "graph_" + test_name,
  });

  // an edge from branch A to branch B means A depends on B.
  // ie. when a change happens on B, A freezes
  this.acceptsMergesFrom = {};
  this.errorLog = [];

  // map of name to branches
  this.branchesMap = {};
  // console.log(createNewGitRepo(test_name));

}

// a dependency from branch A to branch B means A depends on B.
// ie. when a change happens on B, A freezes
Repo.prototype.willMergeInto = function (branchA, branchB, nomessage) {
  if (this.acceptsMergesFrom[branchB.gitGraphBranch.name] === undefined) {
    this.acceptsMergesFrom[branchB.gitGraphBranch.name] = [];
  }
  if (!_.includes(this.acceptsMergesFrom[branchB.gitGraphBranch.name], branchA)) {
    this.acceptsMergesFrom[branchB.gitGraphBranch.name].push(branchA);
  }

  if (!nomessage) {
    this.gitgraph.message("git locks merges " + branchA.gitGraphBranch.name + " " + branchB.gitGraphBranch.name)
  }
}

Repo.prototype.willNotMergeInto = function (branchA, branchB) {
  if (this.acceptsMergesFrom[branchB.gitGraphBranch.name] === undefined) {
    this.acceptsMergesFrom[branchB.gitGraphBranch.name] = [];
  }

  const deps = this.acceptsMergesFrom[branchB.gitGraphBranch.name];
  this.acceptsMergesFrom[branchB.gitGraphBranch.name] = deps.filter(dep => dep !== branchA);
}

Repo.prototype.markLatestCommitAsRevert = function (branch) {
  const commit = branch.gitGraphBranch.commits[branch.gitGraphBranch.commits.length - 1];
  commit.isRevert = true;

  this.gitgraph.message("Command: git locks reverted " + branch.gitGraphBranch.name + " " + commit.sha1);
}

Repo.prototype.getBranchesUpstream = function (branch) {
  if (this.acceptsMergesFrom[branch.name] === undefined) {
    return [];
  }
  return this.acceptsMergesFrom[branch.name].slice();
}

function getBranchByName(repo, branch_name) {

}

Repo.prototype.getBranchesDownstream = function (branch_target) {
  var result = [];
  for (const branch in this.acceptsMergesFrom) {
    if (_.includes(this.acceptsMergesFrom[branch].map(b => b.gitGraphBranch), branch_target)) {
      // find the branch by name
      result.push(this.branchesMap[branch]);
    }
  }
  return result;
}

Repo.prototype.commit = function (gitgraph_settings) {
  this.gitgraph.commit(gitgraph_settings);
  return this;
}

Repo.prototype.branch = function (gitgraph_settings) {
  return new Branch(this, this.gitgraph.branch(gitgraph_settings));
}

function Branch(repo, branch) {
  this.repo = repo;
  this.gitGraphBranch = branch;

  this.repo.branchesMap[branch.name] = this;
}

Branch.prototype.commit = function (args) {
  if (args instanceof String || typeof(args) === "string" || args === undefined) {
    args = {
      message: args,
    }
  }
  if (!args) {
    args = {}
  }

  // Remove outline if no files changed.
  if (args.filesChanged && args.filesChanged.length === 0) {
    args.dotStrokeColor = "white";
  }

  if (!args.tag && args.filesChanged) {
    args.tag = args.filesChanged + "";
  } else if (!args.filesChanged) {
    args.tag = "*.bin";
  }

  try {
    this.repo.verifyCanCommit(this, args ? args.filesChanged : undefined);
  } catch (err) {
    if (err.code === "locks") {
      // If we can't commit, make the same commit, but color it RED.
      args["dotStrokeColor"] = "red";
      args["render_only"] = true;
      this.gitGraphBranch.commit(args);
      err.message = "\nError in commit [" + getBranchHead(this.gitGraphBranch).sha1 + "]\n" + err.message;
      throw err;
    } else {
      throw err;
    }
  }

  this.gitGraphBranch.commit(args);
  return this;
}

Branch.prototype.branch = function (gitgraph_settings) {
  var newbranch = this.gitGraphBranch.branch(gitgraph_settings);
  let newWrappedBranch = new Branch(this.repo, newbranch);

  if (gitgraph_settings.divergent === undefined || gitgraph_settings.divergent === false) {
    this.repo.willMergeInto(this, newWrappedBranch, true);
    this.repo.willMergeInto(newWrappedBranch, this, true);
  }

  return newWrappedBranch;
}

Branch.prototype.merge = function (branch_target, args) {
  this.gitGraphBranch.merge(branch_target.gitGraphBranch, args);
  return this;
}

Repo.prototype.render = function () {
  return this.gitgraph.render();
}

Repo.prototype.getConflictingBranches = function (branch) {
  const divergentCommits = getDivergentCommits(branch);
  const conflictingBranches = [];

  // Two branches conflict if they exists on the same sub-graph of all divergent commits in either branch's history.
  // TODO: Do we gain any information by knowing which branch a divergent commit is in?

  // Get all divergent commits in both branches.
  const branches = Object.values(this.branchesMap);
  for (let i = 0; i < branches.length; i++) {
    const potentialConflictingBranch = branches[i].gitGraphBranch;
    if (potentialConflictingBranch == branch) { continue; }

    const divergentCommitsOther = getDivergentCommits(potentialConflictingBranch);
    const allDivergent = [].concat(divergentCommits).concat(divergentCommitsOther);

    let differentSubtree = false;
    for (let j = 0; j < allDivergent.length; j++) {
      const divergentCommit = allDivergent[j];

      const branchInSubtree = commitDescendsFrom(getBranchHead(branch), divergentCommit);
      const otherBranchInSubtree = commitDescendsFrom(getBranchHead(potentialConflictingBranch), divergentCommit);

      // If one branch is on a separate sub-graph, then that branch is not conflicting.
      if ((branchInSubtree && !otherBranchInSubtree) || (!branchInSubtree && otherBranchInSubtree)) {
        differentSubtree = true;
        break;
      }

      if (!branchInSubtree && !otherBranchInSubtree) {
        throw "Assertion Failed: neither branch exists in the sub-graph for a divergent commit";
      }
    }

    // If one branch is on a separate sub-graph, then that branch is not conflicting.
    if (!differentSubtree) {
      conflictingBranches.push(branches[i]);
    }
  }

  return conflictingBranches;
}

/**
 * Gets all commits labeled as divergent in the history of this branch.
 */
function getDivergentCommits(branch) {

  var candidates = [getBranchHead(branch)];
  let divergentCommits = [];

  while (candidates.length > 0) {
    const candidate = candidates.pop();

    if (candidate.divergent) {
      divergentCommits.push(candidate);
    }

    if (candidate.parentCommit !== undefined && candidate.parentCommit !== null) {
      candidates.push(candidate.parentCommit);
    }
    if (candidate.mergeTargetParentCommit !== undefined && candidate.mergeTargetParentCommit !== null) {
      candidates.push(candidate.mergeTargetParentCommit);
    }
  }

  return divergentCommits;
}

function findFirstCommitsInParentWithFile(commit, filepaths) {
  if (commit.isRevert) {
    // ignore all commits past this point
    return [];
  }

  // undefined == all files touched.
  if (commit.filesChanged === undefined || filepaths === undefined) {
    return [commit];
  }

  for (let i = 0; i < filepaths.length; i++) {
    if (_.includes(commit.filesChanged, filepaths[i])) {
      return [commit];
    }
  }

  let result = [];
  if (commit.parentCommit !== undefined && commit.parentCommit !== null) {
    let items = findFirstCommitsInParentWithFile(commit.parentCommit, filepaths);
    result = result.concat(items);
  }
  if (commit.mergeTargetParentCommit !== undefined) {
    result = result.concat(findFirstCommitsInParentWithFile(commit.mergeTargetParentCommit, filepaths));
  }
  return result;
}

/**
 * @returns True if self descends from target, or is the same commit.
 */
function commitDescendsFrom(self, target) {
  if (target == self) {
    return true;
  }
  if (self === undefined || self === null) {
    return false;
  }
  return commitDescendsFrom(self.parentCommit, target) || (self.mergeTargetParentCommit !== undefined && commitDescendsFrom(self.mergeTargetParentCommit, target));
}

/**
 * Gets the head commit.
 */
function getBranchHead(branch) {
  if (branch.commits.length > 0) {
    return branch.commits[branch.commits.length - 1];
  }

  if (branch.parentCommit != null) {
    return branch.parentCommit;
  }



}

Repo.prototype.verifyCanCommit = function (branch, filepaths) {
  // Get the branches that are conflicting
  const conflictingBranches = this.getConflictingBranches(branch.gitGraphBranch);
  const head = getBranchHead(branch.gitGraphBranch);

  // for each branch, find the most recent change with this file: c.
  // Head must descend from c.
  for (var i = 0; i < conflictingBranches.length; i++) {
    const potential_conflict = conflictingBranches[i].gitGraphBranch;

    const mostRecentCommits = findFirstCommitsInParentWithFile(getBranchHead(potential_conflict), filepaths);

    // HEAD must descend from all mostRecentCommits
    for (var j = 0; j < mostRecentCommits.length; j++) {
      const commitToCheck = mostRecentCommits[j];
      if (!commitDescendsFrom(head, commitToCheck)) {
        const error = new Error("Can't commit due to existing parallel changes on branch [" + potential_conflict.name + "]. \nThe head of Branch [" + branch.gitGraphBranch.name + "] doesn't descend from the commit [" + commitToCheck.sha1 + "] on branch [" + potential_conflict.name + "].");
        error.code = "locks";
        throw error;
      }
    }
  }
}
