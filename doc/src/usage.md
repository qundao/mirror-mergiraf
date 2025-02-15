# Usage

There are two ways to use Mergiraf.
1. You can [**register it as a merge driver**](#registration-as-a-git-merge-driver) in Git so that Mergiraf is directly used during the merge process.
2. Or you can [**invoke it after a merge conflict**](#interactive-use-after-encountering-a-merge-conflict), for it to attempt to solve the conflict.

The first way is recommended as it avoids interrupting your workflow with spurious conflicts.
The second way can be useful for more occasional uses or when changes to Git's configuration are not possible.

## Registration as a Git merge driver

Registering Mergiraf in Git will enable you to benefit from its conflict solving when merging and various other operations, such as rebasing, cherry-picking or even reverting.
For best results, use Git v2.44.0 or newer.

First, add the following section in your `~/.gitconfig` file:

```ini
[merge "mergiraf"]
    name = mergiraf
    driver = mergiraf merge --git %O %A %B -s %S -x %X -y %Y -p %P -l %L

# if you haven't got a global gitattributes file yet
[core]
	attributesfile = ~/.gitattributes
```

Or run:
```console
$ git config --global merge.mergiraf.name mergiraf
$ git config --global merge.mergiraf.driver 'mergiraf merge --git %O %A %B -s %S -x %X -y %Y -p %P -l %L'
$ git config --global core.attributesfile ~/.gitattributes
```

Then, you also need to specify for which sorts of files this merge driver should be used. Add the following lines to your global `~/.gitattributes` file:
```
*.java merge=mergiraf
*.rs merge=mergiraf
*.go merge=mergiraf
*.js merge=mergiraf
*.jsx merge=mergiraf
*.json merge=mergiraf
*.yml merge=mergiraf
*.yaml merge=mergiraf
*.toml merge=mergiraf
*.html merge=mergiraf
*.htm merge=mergiraf
*.xhtml merge=mergiraf
*.xml merge=mergiraf
*.c merge=mergiraf
*.cc merge=mergiraf
*.h merge=mergiraf
*.cpp merge=mergiraf
*.hpp merge=mergiraf
*.cs merge=mergiraf
*.dart merge=mergiraf
*.dts merge=mergiraf
*.scala merge=mergiraf
*.sbt merge=mergiraf
*.ts merge=mergiraf
*.tsx merge=mergiraf
*.py merge=mergiraf
*.php merge=mergiraf
*.sol merge=mergiraf
*.lua merge=mergiraf
*.kt merge=mergiraf
*.rb merge=mergiraf
*.nix merge=mergiraf
```

Or run:
```console
$ mergiraf languages --gitattributes >> ~/.gitattributes
```

This is the complete list of all supported formats - you can of course keep only the ones you need.
If you want to enable Mergiraf only in a certain repository, add the lines above in the `.gitattributes` file at the root of that repository instead, or in `.git/info/attributes` if you don't want it to be tracked in the repository.

### Trying it out

An [example repository](https://codeberg.org/mergiraf/example-repo) is available for you to try out Mergiraf on simple examples:
```console
$ git clone https://codeberg.org/mergiraf/example-repo
$ cd example-repo
$ git merge other-branch
```

### Reviewing Mergiraf's work

When Git invokes Mergiraf to merge a file, it can either:
* successfully merge the file as a line-based merge, just like normal Git would do,
* encounter conflicts in the line-based merge, which it completely solves via its syntax-aware heuristics. In this case it invites you to review its work via the `mergiraf review` command,
* encounter conflicts it cannot solve. In this case, it lets you merge the file manually by leaving conflict markers behind.

If it turns out that Mergiraf's output is unsatisfactory and you would rather use the built-in merge algorithms, abort the operation (such as with `git merge --abort`) and start again with Mergiraf disabled.

### Temporarily disabling Mergiraf

You can disable Mergiraf by setting the `mergiraf` environment variable to 0:
```console
$ mergiraf=0 git rebase origin/master
```

This will fall back on Git's regular merge heuristics, without requiring changes to your configuration.

### Reporting a bad merge

If the output of a merge looks odd, you are encouraged to report it as a bug. The `mergiraf report` command generates an archive containing all necessary information to reproduce the faulty merge.

If the merge did not produce any conflicts, use the merge id (identical to what `mergiraf review` accepts) in Git's output:
```console
$ git rebase origin/master
INFO Mergiraf: Solved 2 conflicts. Review with: mergiraf review geolocation.cpp_o0i2JL8B
Successfully rebased and updated refs/heads/my_branch.
$ mergiraf review geolocation.cpp_o0i2JL8B
$ mergiraf report geolocation.cpp_xyuSMcme
Bug report archive created:

mergiraf_report_6weNKAXO.zip

Please submit it to https://codeberg.org/mergiraf/mergiraf/issues if you are happy with its contents being published,
or reach out privately to a contributor if not.
Thank you for helping Mergiraf improve!
```

If the merge to report has conflicts, use the path to the file instead:
```console
$ mergiraf report src/lib/geolocation.cpp
```

### Compact conflict presentation

By default, Mergiraf aligns the conflicts it outputs to line boundaries to ease their resolution in existing merge tools:

```
<<<<<<< HEAD
<div class="vocab-panel" id="main-panel">
||||||| 15b798c
<div class="vocab-panel">
=======
<div class="vocab-panel" id="root-panel">
>>>>>>> origin/main
```

Because merging is done on syntax trees, it is often able to highlight narrower conflicts.
The option `--compact` (or `-c`) of the `mergiraf merge` command enables a more compact presentation of conflicts which highlights mismatching parts only:

```
<div class="vocab-panel" id=
<<<<<<< HEAD
"main-panel"
||||||| 15b798c
=======
"root-panel"
>>>>>>> origin/main
>
```

The main downside of this mode is that reformatting is often required after resolving conflicts.

## Interactive use after encountering a merge conflict

Say you have encountered a conflict during merge:
```console
$ git merge origin/main
Auto-merging config.yml
CONFLICT (content): Merge conflict in config.yml
Automatic merge failed; fix conflicts and then commit the result.
$ cat config.yml
<<<<<<< HEAD
restaurant:
  tasks:
    plates: 1
    bowls: 2
||||||| 15b798c
tasks:
  plates: 1
  bowls: 2
=======
tasks:
  plates: 1
  bowls: 4
>>>>>>> origin/main
```

You can then run Mergiraf to attempt to solve the conflicts automatically:
```console
$ mergiraf solve config.yml
Solved 1 conflict(s)
```

You can then inspect the result again:
```yaml
restaurant:
  tasks:
    plates: 1
    bowls: 4
```

You can then mark the conflict as solved with `git add` and continue merging with `git merge --continue`.
