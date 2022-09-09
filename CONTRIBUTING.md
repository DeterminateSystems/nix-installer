# Contributing to Harmonic

We're excited to see what you'd like to contribute to Harmonic!

Regardless of what (or how much) you contribute to Harmonic, we value your time
and energy trying to improve the tool.

In order to ensure we all have a good experience, please review this document
if you have any questions about the process.

**Regular Rust committer?** Contributing to Harmonic should feel similar to
contributing to other serious Rust ecosystem projects. You may already know
the process and expectations of you, this document shouldn't contain any
surprises.


# What kinds of contributions are needed?

Riff can benefit from all kinds of contributions:

* Bug reports
* Code improvements
* Registry additions
* Dependency updates or dependency feature trimming
* New features (Please create an issue first!)
* Documentation
* Graphical/visual asset improvement
* Kind words or recommendation on your own site, repo, stream, or social media
  account
* Onboarding others to using Riff


# What are the expectations you can have of the maintainers?

You can expect us to:

* Follow the [Contributor Covenant](CODE_OF_CONDUCT.md), just like you
* Help diagnose bug reports (for supported platforms using supported
  languages)
* Give constructive feedback on pull requests
* Merge pull requests which:
    + Have been approved of by at least 1 maintainer
    + Pass all tests
    + Have no complex conflicts with in-flight high priority work

The maintainers of this project use a separate issue tracker for some internal
tasks. Unfortunately, the contents of this tracker is not publicly visible as
it may contain sensitive or confidential data. Our maintainers will endeavor to
ensure you are not 'left out' of the discussion about your contributions.


# What kind of expectations do the maintainers have from you?

We expect you to:

* Follow the [Contributor Covenant](CODE_OF_CONDUCT.md), just like them
* Make an earnest attempt to follow the contribution process described in this
  document
* Update bug reports with a solution, if you find one before we do
* Do your best to follow existing conventions
* Reflect maintainer feedback if you are able
* Declare if you need to abandon a PR so someone else can shepherd it


# How exactly does the contribution process work?

Here are how to do various kinds of contributions.


## Bug Reports

Create an issue on [the issue page](https://github.com/DeterminateSystems/riff/issues).

It should contain:

1. Your OS (Linux, Mac) and architecture (x86_64, aarch64)
2. Your Harmonic version (`harmonic --version`)
3. The thing you tried to run (eg `harmonic`)
4. What happened (the output of the command, please)
5. What you expected to happen
6. If you tried to fix it, what did you try?


## Code/Documentation improvement

For **minor** fixes, documentation, or changes which **do not** have a
tangible impact on user experience, feel free to open a
[pull request](https://github.com/DeterminateSystems/riff/pulls) directly.

If the code improvement is not minor, such as new features or user facing
changes, an [issue](https://github.com/DeterminateSystems/riff/issues)
proposing the change is **required** for non-maintainers.

Please:

* Write civil commit messages, it's ok if they are simple like `fmt`
  or `formatting`
* Follow existing conventions and style within the code the best you can
* Describe in your PR the problem and solution so reviewers don't need to
  rebuild much context
* Run `nix check` and `nix build`


## Non-code contributions

Please open an [issue](https://github.com/DeterminateSystems/riff/issues)
to chat about your contribution and figure out how to best integrate it into
the project.


# Who maintains Harmonic and why?

Riff is maintained by [Determinate Systems](https://determinate.systems/) in
an effort to explore Nix installer ideas.

Determinate Systems has no plans to monetize or relicense Harmonic. If your
enterprise requires a support contact in order to adopt a tool, please contact
Determinate Systems and something can be worked out.
