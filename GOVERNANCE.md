# Mergiraf's governance model

This document describes the different roles for participants to the Mergiraf project.

## Roles

At any point in time, a person is either a **Contributor**, a **Developer** or a **Publisher**.
By "member" we mean a Developer or Publisher. The list of Developers and Publishers is recorded in `members.yml` in this repository.

The contents of `members.yml` is mirrored by the [Codeberg teams of the Mergiraf organization](https://codeberg.org/org/mergiraf/teams). (Publishers are responsible for such updates).
All project members are made public.

## Rights and responsibilities

* Contributors can use Mergiraf and make changes to it as specified by the license.
* Developers can merge pull requests made by others and triage issues, in addition to the above. They are responsible for processing the pull request and issue backlog.
* Publishers can publish new releases and merge their own pull requests, in addition to the above. They are also responsible for administering the project's Codeberg organization and repository following the principles outlined in this document.

## Onboarding

A Contributor can apply to become a Developer by opening a pull request making the corresponding change in `members.yml`. The pull request is merged or rejected following a vote open to existing Developers and Publishers.
A Developer can apply to become a Publisher by opening a pull request making the corresponding change in `members.yml`. The pull request is merged or rejected following a vote open to existing Publishers.

## Offboarding

A Developer can step down to being a Contributor and a Publisher can step down to being a Developer by making the appropriate pull request. The pull request is accepted without a vote if it is opened by the person subject to the change.
A Developer who hasn't made use of Developer rights for 2 years automatically becomes a Contributor.
A Publisher who hasn't made use of Developer or Publisher rights for 1 year and isn't the only Publisher automatically becomes a Developer.

## Voting procedure

Votes are held in pull requests which change this document or `members.yml` and are not covered by the exceptions mentioned above. Eligible voters can cast three votes:
* Support, +1
* Accept, 0
* Reject, -3

Votes are cast publicly by commenting on the pull request. The vote passes if the sum of votes is nonnegative. Votes last for at least a week.

## Making changes to this document

Changes to this document are made by pull requests, where a vote open to Developers and Publishers is held. The person proposing the change is able to cast a vote.

## Known issues

The mergiraf.org domain is held by Antonin Delpeuch. A solution to share its ownership with all Publishers would be welcome.

This document is available under a [CC0 license](https://creativecommons.org/public-domain/cc0/). No rights reserved.
