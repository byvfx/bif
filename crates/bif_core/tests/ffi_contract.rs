#[path = "_helpers.rs"]
#[allow(dead_code)]
mod helpers;

use std::ffi::{c_char, CStr, CString};
use std::ptr;

use bif_core::usd::UsdStage;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum UsdBridgeErrorCode {
    Success = 0,
    NullPointer = 1,
    FileNotFound = 2,
    InvalidStage = 3,
    InvalidPrim = 4,
    OutOfMemory = 5,
    Unknown = 99,
}

#[repr(C)]
struct UsdBridgeStageRaw {
    _private: [u8; 0],
}

extern "C" {
    fn usd_bridge_open_stage(
        path: *const c_char,
        out_stage: *mut *mut UsdBridgeStageRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_close_stage(stage: *mut UsdBridgeStageRaw);

    fn usd_bridge_layer_export_as_string(
        stage: *const UsdBridgeStageRaw,
        layer_identifier: *const c_char,
        out_text: *mut *const c_char,
    ) -> UsdBridgeErrorCode;
}

#[test]
fn layer_export_as_string_pointer_is_reused_per_thread() {
    let fixture = helpers::LayeredStageFixture::new("ffi_export_contract");
    helpers::write(
        &fixture.working,
        &format!(
            "#usda 1.0\n\n{}\n",
            "# export-buffer contract padding\n".repeat(16)
        ),
    );
    let public_stage = UsdStage::open(&fixture.root).expect("open public stage");
    let stack = public_stage.get_layer_stack().expect("layer stack");
    let asset_id = stack
        .layers
        .iter()
        .find(|l| l.identifier.ends_with("asset.usda"))
        .map(|l| l.identifier.clone())
        .expect("asset layer");
    let working_id = stack
        .layers
        .iter()
        .find(|l| l.identifier.ends_with("working.usda"))
        .map(|l| l.identifier.clone())
        .expect("working layer");

    let root = CString::new(fixture.root.to_string_lossy().as_bytes()).expect("root path");
    let asset = CString::new(asset_id).expect("asset id");
    let working = CString::new(working_id).expect("working id");

    let mut stage = ptr::null_mut();
    let open_code = unsafe { usd_bridge_open_stage(root.as_ptr(), &mut stage) };
    assert_eq!(open_code, UsdBridgeErrorCode::Success);
    assert!(!stage.is_null());

    let mut first_ptr = ptr::null();
    let first_code =
        unsafe { usd_bridge_layer_export_as_string(stage, asset.as_ptr(), &mut first_ptr) };
    assert_eq!(first_code, UsdBridgeErrorCode::Success);
    assert!(!first_ptr.is_null());
    let first_text = unsafe { CStr::from_ptr(first_ptr).to_string_lossy().into_owned() };
    assert!(
        first_text.contains("UsdPreviewSurface"),
        "expected asset layer text, got {first_text:?}"
    );

    let mut second_ptr = ptr::null();
    let second_code =
        unsafe { usd_bridge_layer_export_as_string(stage, working.as_ptr(), &mut second_ptr) };
    assert_eq!(second_code, UsdBridgeErrorCode::Success);
    assert_eq!(
        first_ptr, second_ptr,
        "thread-local export buffer pointer changed"
    );

    let overwritten_text = unsafe { CStr::from_ptr(first_ptr).to_string_lossy().into_owned() };
    assert_ne!(first_text, overwritten_text);
    assert_eq!(overwritten_text, unsafe {
        CStr::from_ptr(second_ptr).to_string_lossy().into_owned()
    });

    unsafe { usd_bridge_close_stage(stage) };
}
