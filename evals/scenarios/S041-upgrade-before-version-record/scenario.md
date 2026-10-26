# S041 Upgrade a project set up before the kit recorded its version
- Job: J1
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a project set up with an older kit, who has just rebuilt `delegate` from a newer kit. The project was set up before upgrades existed, like every project today.

## Starting state
A project set up by a kit with no version record. Its skills are exact copies of what that kit
shipped, except one the owner edited.

## Steps
1. `delegate upgrade`.

## Passes when
- The untouched kit files are updated, the edited one is kept and named, as in S038 and S039.
- Afterwards the project records its kit version, so the next upgrade works like S038.
- It doesn't ask the user which version they had.

## If this fails
Every project that exists today can't upgrade, or an upgrade treats all of them as hand-edited
and updates nothing.
