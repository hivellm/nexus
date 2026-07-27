# parking_lot::RwLock does not allow recursive acquire

**Category**: concurrency
**Tags**: none

## Description

parking_lot::RwLock is NOT reentrant — chained write/read acquisitions in a single expression deadlock
