# FIXED: CREATE Command "Duplication" Bug

**Status**: ✅ **FIXED** - Was actually a MATCH bug, not CREATE

**Priority**: 🟢 **RESOLVED**

## Problem Description

Tests were showing that CREATE commands appeared to be creating **2 nodes instead of 1**.

## Root Cause - NOT a CREATE bug!

After extensive investigation with detailed debugging, discovered the REAL bug:

**The `execute_node_by_label()` function was treating `label_id == 0` as a special case for "scan all nodes".**

However, `label_id == 0` is a **VALID label ID** (it's the first label created by the catalog)!

### The Bug

In `nexus-core/src/executor/mod.rs` (line ~1190):

```rust
fn execute_node_by_label(&self, label_id: u32) -> Result<Vec<Value>> {
    let bitmap = if label_id == 0 {
        // ❌ BUG: This treated label_id=0 as "scan all"
        // But label_id=0 is the FIRST valid label!
        let total_nodes = self.store.node_count();
        let mut all_nodes = roaring::RoaringBitmap::new();
        for node_id in 0..total_nodes.min(u32::MAX as u64) {
            all_nodes.insert(node_id as u32);
        }
        all_nodes
    } else {
        self.label_index.get_nodes(label_id)?
    };
    // ...
}
```

### What Actually Happened

1. CREATE one node with label "X" → label_id=0, node_id=0 ✅
2. Label index correctly stores: {label_id=0 → [node_id=0]} ✅
3. MATCH (n:X) → label_id=0
4. execute_node_by_label sees label_id==0 → enters "scan all" mode ❌
5. "Scan all" uses `node_count()` which returns `next_node_id` (already incremented to 1)
6. Scans node IDs 0..1 → returns nodes [0, 1]
7. But node_id=1 doesn't exist! It returns whatever garbage/old data is there ❌

Result: Appeared to return 2 nodes when only 1 was created.

## Solution

**Remove the special case for `label_id == 0`** - always use the label_index:

```rust
fn execute_node_by_label(&self, label_id: u32) -> Result<Vec<Value>> {
    // ✅ Always use label_index - label_id 0 is valid (it's the first label)
    let bitmap = self.label_index.get_nodes(label_id)?;

    let mut results = Vec::new();
    for node_id in bitmap.iter() {
        // Skip deleted nodes
        if let Ok(node_record) = self.store.read_node(node_id as u64) {
            if node_record.is_deleted() {
                continue;
            }
        }
        match self.read_node_as_value(node_id as u64)? {
            Value::Null => continue,
            value => results.push(value),
        }
    }
    Ok(results)
}
```

## Files Modified

- `nexus-core/src/executor/mod.rs` (line ~1187-1209): Removed special case for label_id=0

## Test Results

- ✅ All CREATE tests now pass
- ✅ No duplication when creating nodes
- ✅ label_id=0 works correctly
- ✅ Revealed actual WHERE IN operator bugs (which were masked before)

## Impact

This fix:

- ✅ Resolves all "CREATE duplication" test failures
- ✅ Enables proper testing of WHERE clause functionality
- ✅ Fixes fundamental MATCH behavior for first label
- ⚠️ Reveals real WHERE IN bugs that need separate fixes
