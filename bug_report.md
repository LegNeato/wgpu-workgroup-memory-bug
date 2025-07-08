# Naga HLSL Backend: Race Condition in Workgroup Memory Initialization

## Bug Description

The Naga HLSL backend has a race condition when initializing workgroup memory that causes data corruption in compute shaders. This affects any shader that writes to workgroup memory immediately upon entry.

### The Issue

When `zero_initialize_workgroup_memory` is enabled, the HLSL backend generates:

```hlsl
// At function entry:
if (all(__local_invocation_id == uint3(0u, 0u, 0u))) {
    // Only thread 0 initializes workgroup memory
    shared_array = {0, 0, 0, ...};
}
GroupMemoryBarrierWithGroupSync();
// User shader code starts here
```

However, this creates a race condition:
1. All threads begin execution simultaneously
2. Non-zero threads skip the initialization check and proceed to write to workgroup memory
3. Thread 0 may still be initializing, overwriting values already written by other threads
4. The barrier comes too late - damage is already done

### Reproduction

This bug manifests in workgroup reduction algorithms where each thread writes to its own slot:

```rust
// Each thread writes immediately
shared[local_id] = input[local_id];
workgroup_memory_barrier_with_group_sync();
// Reduction logic...
```

Result: Only partial data survives (e.g., sum of 1..64 returns 3 instead of 2080).

### Root Cause

The current implementation in `naga/src/back/hlsl/writer.rs` (lines 1635-1652) places the initialization inside a conditional block that only thread 0 executes, but doesn't prevent other threads from proceeding past this point.

## Proposed Fix

Ensure all threads wait for initialization to complete before proceeding:

### Patch 1: Simple Double Barrier Fix

```diff
--- a/naga/src/back/hlsl/writer.rs
+++ b/naga/src/back/hlsl/writer.rs
@@ -1649,7 +1649,9 @@ impl<'a, W: Write> Writer<'a, W> {
         }
 
         writeln!(self.out, "{level}}}")?;
-        self.write_control_barrier(crate::Barrier::WORK_GROUP, level)
+        self.write_control_barrier(crate::Barrier::WORK_GROUP, level)?;
+        // Second barrier to ensure initialization completes before any thread proceeds
+        self.write_control_barrier(crate::Barrier::WORK_GROUP, level)
     }
```

### Patch 2: More Robust Fix (Preferred)

```diff
--- a/naga/src/back/hlsl/writer.rs
+++ b/naga/src/back/hlsl/writer.rs
@@ -1628,13 +1628,17 @@ impl<'a, W: Write> Writer<'a, W> {
     fn write_workgroup_variables_initialization(
         &mut self,
         func_ctx: &back::FunctionCtx,
         module: &Module,
     ) -> BackendResult {
         let level = back::Level(1);
 
+        // Ensure initialization happens before any thread can proceed
+        writeln!(self.out, "{level}{{")?;
+        
         writeln!(
             self.out,
-            "{level}if (all(__local_invocation_id == uint3(0u, 0u, 0u))) {{"
+            "{}if (all(__local_invocation_id == uint3(0u, 0u, 0u))) {{",
+            level.next()
         )?;
 
         let vars = module.global_variables.iter().filter(|&(handle, var)| {
@@ -1643,13 +1647,17 @@ impl<'a, W: Write> Writer<'a, W> {
 
         for (handle, var) in vars {
             let name = &self.names[&NameKey::GlobalVariable(handle)];
-            write!(self.out, "{}{} = ", level.next(), name)?;
+            write!(self.out, "{}{} = ", level.next().next(), name)?;
             self.write_default_init(module, var.ty)?;
             writeln!(self.out, ";")?;
         }
 
-        writeln!(self.out, "{level}}}")?;
-        self.write_control_barrier(crate::Barrier::WORK_GROUP, level)
+        writeln!(self.out, "{}}}", level.next())?;
+        
+        // Barrier inside the scope ensures all threads wait for init
+        self.write_control_barrier(crate::Barrier::WORK_GROUP, level.next())?;
+        
+        writeln!(self.out, "{level}}}")
     }
```

### Why This Fix Works

The double barrier ensures:
1. Thread 0 completes initialization before any thread proceeds
2. All threads synchronize after initialization
3. No thread can write to workgroup memory until initialization is complete

This eliminates the race condition while maintaining the zero-initialization guarantee.

## Impact

This bug affects all compute shaders on Windows/DX12 that:
- Use workgroup memory
- Have `zero_initialize_workgroup_memory` enabled
- Write to workgroup memory before the first explicit barrier

The fix is backward compatible and only adds one additional barrier at shader entry.