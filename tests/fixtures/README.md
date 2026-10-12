# Put Back parser fixture

`put_back.ds_store` was generated with the independent Python `ds-store==1.3.1`
writer. It contains two fictional Put Back entries (`renamed.txt` and `雪.txt`)
and an unrelated `Iloc` blob. It contains no metadata from a user's Trash.

The parser tests read this fixture without writing it, and mutate in-memory copies
to exercise bounds checks and stale bytes outside the active tree.
