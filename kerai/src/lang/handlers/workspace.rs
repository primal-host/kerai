use crate::lang::machine::Machine;
use crate::lang::ptr::Ptr;

/// Push the workspace library marker onto the stack.
pub fn workspace_lib(m: &mut Machine) -> Result<(), String> {
    m.push(Ptr::library("workspace"));
    Ok(())
}

/// `workspace list` — query workspaces for the current user, push as workspace_list.
/// Note: This is a synchronous handler. The actual DB query happens in the async
/// eval route. Here we push a marker that the eval layer will resolve.
pub fn ws_list(m: &mut Machine) -> Result<(), String> {
    // Push a workspace_list request marker.
    // The serve layer will detect this and fill in actual data.
    m.push(Ptr {
        kind: "workspace_list_request".into(),
        ref_id: m.user_id.to_string(),
        meta: serde_json::Value::Null,
        id: 0,
    });
    Ok(())
}

/// `workspace load` — pop an int (selection number) from the stack.
/// The serve layer resolves the actual workspace switch.
pub fn ws_load(m: &mut Machine) -> Result<(), String> {
    let selector = m.pop().ok_or("workspace load: need a selection number")?;

    match selector.as_int() {
        Some(n) => {
            m.push(Ptr {
                kind: "workspace_load_request".into(),
                ref_id: n.to_string(),
                meta: serde_json::Value::Null,
                id: 0,
            });
            Ok(())
        }
        None => {
            m.push(selector);
            Err("workspace load: top of stack must be an integer".into())
        }
    }
}

/// `workspace new` — pop a text name from the stack, create workspace.
pub fn ws_new(m: &mut Machine) -> Result<(), String> {
    let name = m.pop().ok_or("workspace new: need a name on the stack")?;

    if name.kind != "text" {
        m.push(name);
        return Err("workspace new: top of stack must be text".into());
    }

    m.push(Ptr {
        kind: "workspace_new_request".into(),
        ref_id: name.ref_id,
        meta: serde_json::Value::Null,
        id: 0,
    });
    Ok(())
}

/// `workspace save` — name the current (anonymous) workspace.
pub fn ws_save(m: &mut Machine) -> Result<(), String> {
    let name = m.pop().ok_or("workspace save: need a name on the stack")?;

    if name.kind != "text" {
        m.push(name);
        return Err("workspace save: top of stack must be text".into());
    }

    m.push(Ptr {
        kind: "workspace_save_request".into(),
        ref_id: name.ref_id,
        meta: serde_json::Value::Null,
        id: 0,
    });
    Ok(())
}
