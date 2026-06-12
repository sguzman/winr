use tracing::{debug, instrument, trace, warn};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, CUIAutomation8, IUIAutomation, IUIAutomationElement, IUIAutomationElementArray,
    IUIAutomationInvokePattern, IUIAutomationTreeWalker, IUIAutomationValuePattern,
    TreeScope_Subtree, UIA_InvokePatternId, UIA_ValuePatternId,
};
use windows::core::{BSTR, Error as WindowsError};
use winr_types::{
    Rect, UiaActionRequest, UiaActionResult, UiaElementInfo, UiaFindRequest, UiaFindResponse,
    UiaSelector, UiaSetTextRequest, UiaTreeMode, UiaTreeRequest, UiaTreeResponse, WindowInfo,
    WinrError, WinrResult, format_hwnd,
};

use crate::{
    config::enforce_input_permission, parse_selector_hwnd,
    security::enforce_integrity_level_for_pid, window_info,
};

#[instrument(skip(request))]
pub fn uia_tree(request: &UiaTreeRequest) -> WinrResult<UiaTreeResponse> {
    let window = window_info(&request.window)?;
    let hwnd = parse_selector_hwnd(&window.hwnd);
    let _com = ComGuard::initialize()?;
    let automation = create_automation()?;
    let root = automation_root(&automation, hwnd, &window)?;
    let mode = request.mode.unwrap_or(UiaTreeMode::Control);
    let walker = tree_walker(&automation, mode)?;
    let max_depth = request.max_depth.unwrap_or(4);

    debug!(
        hwnd = %window.hwnd,
        max_depth,
        mode = ?mode,
        "building UI Automation tree"
    );

    let root = collect_tree(&walker, &root, 0, max_depth)?;
    Ok(UiaTreeResponse { window, mode, root })
}

#[instrument(skip(request))]
pub fn uia_find(request: &UiaFindRequest) -> WinrResult<UiaFindResponse> {
    let window = window_info(&request.window)?;
    let hwnd = parse_selector_hwnd(&window.hwnd);
    let _com = ComGuard::initialize()?;
    let automation = create_automation()?;
    let root = automation_root(&automation, hwnd, &window)?;
    let matches = find_matching_elements(&root, &request.element)?;

    debug!(
        hwnd = %window.hwnd,
        match_count = matches.len(),
        "resolved UI Automation selector"
    );

    Ok(UiaFindResponse { window, matches })
}

#[instrument(skip(request))]
pub fn uia_invoke(request: &UiaActionRequest) -> WinrResult<UiaActionResult> {
    let (window, element, native) = resolve_single_element(&request.window, &request.element)?;
    enforce_input_permission(&window, "uia_invoke")?;
    enforce_integrity_level_for_pid(window.pid, "uia_invoke")?;
    debug!(
        hwnd = %window.hwnd,
        element_name = ?element.name,
        automation_id = ?element.automation_id,
        "invoking UI Automation element"
    );

    let pattern = unsafe {
        native
            .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
            .map_err(windows_error)?
    };
    unsafe { pattern.Invoke().map_err(windows_error)? };

    Ok(UiaActionResult {
        action: "invoke".to_string(),
        window,
        element,
        details: None,
    })
}

#[instrument(skip(request))]
pub fn uia_set_text(request: &UiaSetTextRequest) -> WinrResult<UiaActionResult> {
    let (window, element, native) = resolve_single_element(&request.window, &request.element)?;
    enforce_input_permission(&window, "uia_set_text")?;
    enforce_integrity_level_for_pid(window.pid, "uia_set_text")?;
    debug!(
        hwnd = %window.hwnd,
        element_name = ?element.name,
        automation_id = ?element.automation_id,
        text_len = request.text.chars().count(),
        "setting UI Automation value"
    );

    let pattern = unsafe {
        native
            .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            .map_err(windows_error)?
    };
    let value = BSTR::from(request.text.as_str());
    unsafe { pattern.SetValue(&value).map_err(windows_error)? };

    Ok(UiaActionResult {
        action: "set_text".to_string(),
        window,
        element,
        details: Some(request.text.clone()),
    })
}

fn resolve_single_element(
    window_selector: &winr_types::WindowSelector,
    element_selector: &UiaSelector,
) -> WinrResult<(WindowInfo, UiaElementInfo, IUIAutomationElement)> {
    let window = window_info(window_selector)?;
    let hwnd = parse_selector_hwnd(&window.hwnd);
    let _com = ComGuard::initialize()?;
    let automation = create_automation()?;
    let root = automation_root(&automation, hwnd, &window)?;
    let native = resolve_native_element(&root, element_selector)?;
    let element = describe_element(&native)?;
    Ok((window, element, native))
}

fn resolve_native_element(
    root: &IUIAutomationElement,
    selector: &UiaSelector,
) -> WinrResult<IUIAutomationElement> {
    if !selector.has_criteria() {
        return Err(WinrError::Unsupported {
            message: "at least one UI Automation selector flag is required".to_string(),
        });
    }

    let matches = find_matching_native_elements(root, selector)?;
    match matches.len() {
        0 => Err(WinrError::UiaElementNotFound),
        1 => Ok(matches
            .into_iter()
            .next()
            .expect("single UIA match present")),
        count => Err(WinrError::AmbiguousUiaElement {
            count,
            matches: matches
                .into_iter()
                .map(|element| describe_element(&element))
                .collect::<WinrResult<Vec<_>>>()?,
        }),
    }
}

fn find_matching_elements(
    root: &IUIAutomationElement,
    selector: &UiaSelector,
) -> WinrResult<Vec<UiaElementInfo>> {
    find_matching_native_elements(root, selector)?
        .into_iter()
        .map(|element| describe_element(&element))
        .collect()
}

fn find_matching_native_elements(
    root: &IUIAutomationElement,
    selector: &UiaSelector,
) -> WinrResult<Vec<IUIAutomationElement>> {
    let all = subtree_elements(root)?;
    let mut matches = Vec::new();

    for element in all {
        let described = describe_element(&element)?;
        if selector.matches(&described) {
            trace!(
                automation_id = ?described.automation_id,
                name = ?described.name,
                class_name = ?described.class_name,
                "matched UI Automation element"
            );
            matches.push(element);
        }
    }

    Ok(matches)
}

fn subtree_elements(root: &IUIAutomationElement) -> WinrResult<Vec<IUIAutomationElement>> {
    let automation = create_automation()?;
    let condition = unsafe { automation.CreateTrueCondition().map_err(windows_error)? };
    let array = unsafe {
        root.FindAll(TreeScope_Subtree, &condition)
            .map_err(windows_error)?
    };
    array_to_vec(&array)
}

fn array_to_vec(array: &IUIAutomationElementArray) -> WinrResult<Vec<IUIAutomationElement>> {
    let len = unsafe { array.Length().map_err(windows_error)? };
    let mut elements = Vec::with_capacity(len.max(0) as usize);
    for index in 0..len {
        let element = unsafe { array.GetElement(index).map_err(windows_error)? };
        elements.push(element);
    }
    Ok(elements)
}

fn collect_tree(
    walker: &IUIAutomationTreeWalker,
    element: &IUIAutomationElement,
    depth: u32,
    max_depth: u32,
) -> WinrResult<UiaElementInfo> {
    let mut node = describe_element(element)?;
    if depth >= max_depth {
        return Ok(node);
    }

    let mut children = Vec::new();
    let mut current = walker_first_child(walker, element);
    while let Some(child) = current {
        children.push(collect_tree(walker, &child, depth + 1, max_depth)?);
        current = walker_next_sibling(walker, &child);
    }
    node.children = children;
    Ok(node)
}

fn walker_first_child(
    walker: &IUIAutomationTreeWalker,
    element: &IUIAutomationElement,
) -> Option<IUIAutomationElement> {
    unsafe { walker.GetFirstChildElement(element).ok() }
}

fn walker_next_sibling(
    walker: &IUIAutomationTreeWalker,
    element: &IUIAutomationElement,
) -> Option<IUIAutomationElement> {
    unsafe { walker.GetNextSiblingElement(element).ok() }
}

fn tree_walker(
    automation: &IUIAutomation,
    mode: UiaTreeMode,
) -> WinrResult<IUIAutomationTreeWalker> {
    match mode {
        UiaTreeMode::Control => unsafe { automation.ControlViewWalker().map_err(windows_error) },
        UiaTreeMode::Raw => unsafe { automation.RawViewWalker().map_err(windows_error) },
    }
}

fn automation_root(
    automation: &IUIAutomation,
    hwnd: HWND,
    window: &WindowInfo,
) -> WinrResult<IUIAutomationElement> {
    unsafe {
        automation.ElementFromHandle(hwnd).map_err(|error| {
            windows_operation_error(
                error,
                format!("ElementFromHandle failed for {}", window.hwnd),
            )
        })
    }
}

fn describe_element(element: &IUIAutomationElement) -> WinrResult<UiaElementInfo> {
    let hwnd = unsafe { element.CurrentNativeWindowHandle().ok() }
        .and_then(non_null_hwnd)
        .map(|value| format_hwnd(value.0 as usize as isize));
    let name = unsafe { element.CurrentName().ok() }.and_then(bstr_to_option);
    let automation_id = unsafe { element.CurrentAutomationId().ok() }.and_then(bstr_to_option);
    let class_name = unsafe { element.CurrentClassName().ok() }.and_then(bstr_to_option);
    let localized_control_type =
        unsafe { element.CurrentLocalizedControlType().ok() }.and_then(bstr_to_option);
    let control_type = unsafe { element.CurrentControlType().ok() }.map(|value| value.0);
    let enabled = unsafe { element.CurrentIsEnabled().ok() }.map(|value| value.as_bool());
    let rect = unsafe { element.CurrentBoundingRectangle().ok() }.and_then(rect_to_option);

    Ok(UiaElementInfo {
        hwnd,
        automation_id,
        name,
        class_name,
        localized_control_type,
        control_type,
        enabled,
        rect,
        children: Vec::new(),
    })
}

fn create_automation() -> WinrResult<IUIAutomation> {
    unsafe {
        CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
            .or_else(|error| {
                warn!(%error, "CUIAutomation8 unavailable, falling back to CUIAutomation");
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            })
            .map_err(windows_error)
    }
}

fn rect_to_option(rect: RECT) -> Option<Rect> {
    if rect.left == 0 && rect.top == 0 && rect.right == 0 && rect.bottom == 0 {
        return None;
    }

    Some(Rect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    })
}

fn bstr_to_option(value: BSTR) -> Option<String> {
    let text = value.to_string();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn non_null_hwnd(hwnd: HWND) -> Option<HWND> {
    if hwnd.0.is_null() { None } else { Some(hwnd) }
}

fn windows_error(error: WindowsError) -> WinrError {
    WinrError::Unsupported {
        message: error.to_string(),
    }
}

fn windows_operation_error(error: WindowsError, context: String) -> WinrError {
    WinrError::Unsupported {
        message: format!("{context}: {error}"),
    }
}

struct ComGuard {
    should_uninitialize: bool,
}

impl ComGuard {
    fn initialize() -> WinrResult<Self> {
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if hr.is_ok() {
            trace!("initialized COM for UI Automation");
            return Ok(Self {
                should_uninitialize: true,
            });
        }

        let error = WindowsError::from(hr);
        let message = error.to_string();
        if message.contains("changed mode") {
            debug!(%error, "COM already initialized in a different apartment");
            Ok(Self {
                should_uninitialize: false,
            })
        } else {
            Err(windows_error(error))
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.should_uninitialize {
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_matches_uia_nodes() {
        let selector = UiaSelector {
            name: Some("username".to_string()),
            class_name: Some("edit".to_string()),
            enabled: Some(true),
            ..UiaSelector::default()
        };
        let node = UiaElementInfo {
            hwnd: None,
            automation_id: Some("user".to_string()),
            name: Some("Username".to_string()),
            class_name: Some("Edit".to_string()),
            localized_control_type: Some("edit".to_string()),
            control_type: Some(50004),
            enabled: Some(true),
            rect: None,
            children: Vec::new(),
        };

        assert!(selector.matches(&node));
    }

    #[test]
    fn empty_rect_is_filtered() {
        assert!(rect_to_option(RECT::default()).is_none());
    }

    #[test]
    fn bstr_conversion_ignores_empty_strings() {
        assert!(bstr_to_option(BSTR::from("   ")).is_none());
        assert_eq!(bstr_to_option(BSTR::from("OK")).as_deref(), Some("OK"));
    }
}
