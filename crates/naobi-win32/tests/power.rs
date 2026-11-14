//! Tier 2: the real power request. `SystemExecutionState` shows it without an elevated `powercfg`
//! (the literal `powercfg /requests` check is manual at Gate 3, RK-09).
use naobi_win32::power::{execution_state, set_request_now};

const SYSTEM: u32 = 0x1;
const DISPLAY: u32 = 0x2;

// @R-019
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r019_power_request_shows_in_the_system_execution_state_and_clears() {
    // Another program (a player, a download) may hold the system awake itself; only what this
    // request changes can be asserted.
    let before = execution_state();
    assert!(set_request_now(true, false));
    let held = execution_state();
    assert_ne!(held & SYSTEM, 0, "the system-required request is visible");
    assert!(set_request_now(true, true));
    assert_ne!(execution_state() & DISPLAY, 0, "display required joins it");
    // Windows counts repeated requests: applying again must not need a second clear.
    assert!(set_request_now(true, true));
    assert!(set_request_now(true, true));
    assert!(set_request_now(true, false));
    if before & DISPLAY == 0 {
        assert_eq!(execution_state() & DISPLAY, 0, "display released again");
    }
    assert!(set_request_now(false, false));
    if before & SYSTEM == 0 {
        assert_eq!(execution_state() & SYSTEM, 0, "released: Windows may sleep");
    }
    // Releasing twice is harmless.
    assert!(set_request_now(false, false));
}
