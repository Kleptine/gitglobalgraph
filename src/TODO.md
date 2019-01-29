# TODO

## Global Graph
 - Diverging branches.
 - Server executable / setup.
 - Uninstall workflow.

Future:
 - Index Synchronizing
 - Working Tree Synchronizing
 - Separate Global Graph from ConflictsDetection

## Conflicts Detection
Features:
 - Handle resets.
 - Only check files with git attributes set.
 - Git CLI for interactive query of GG: `git globalgraph conflicts`

Tests:
 - Test conflicts are returned on both branches if a developer force pushes a conflict.
 - Rebasing workflow
 - Merging workflow

Unsupported Workflows:
 - When branch references are manually changed, block if it would create conflicts. Currently difficult without a git hook for reference changes.


