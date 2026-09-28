use super::*;
use crate::api::schema::{Method, PaneMoveDestination};
use crossterm::event::{MouseButton, MouseEventKind};

fn two_workspace_snapshot() -> ClientShellSnapshot {
    let mut projected = snapshot();
    projected.tabs[0].label = "alpha".into();
    projected.tabs[0].custom_label = true;

    let mut workspace = projected.workspaces[0].clone();
    workspace.workspace_id = "ws_2".into();
    workspace.active_tab_id = "tab_2".into();
    workspace.label = "second".into();
    workspace.focused = false;
    projected.workspaces.push(workspace);

    let mut tab = projected.tabs[0].clone();
    tab.tab_id = "tab_2".into();
    tab.workspace_id = "ws_2".into();
    tab.label = "beta".into();
    tab.focused = false;
    projected.tabs.push(tab);

    let mut pane = projected.panes[0].clone();
    pane.pane_id = "pane_2".into();
    pane.workspace_id = "ws_2".into();
    pane.tab_id = "tab_2".into();
    pane.focused = false;
    projected.panes.push(pane);
    projected
}

fn drag_state(projected: ClientShellSnapshot) -> ClientShellState {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    state.compose(106, 24).expect("composed frame");
    state
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> crate::raw_input::RawInputEvent {
    crate::raw_input::RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::empty(),
    })
}

fn endpoint_methods(outcome: &ClientShellInput) -> Vec<&Method> {
    outcome
        .actions
        .iter()
        .filter_map(|action| match action {
            ClientShellAction::Endpoint { request, .. } => Some(&request.method),
            _ => None,
        })
        .collect()
}

fn cell_bg(frame: &FrameData, x: u16, y: u16) -> u32 {
    frame.cells[(y as usize) * (frame.width as usize) + (x as usize)].bg
}

#[test]
fn tab_drag_to_new_workspace_button_moves_focused_pane() {
    let mut state = drag_state(two_workspace_snapshot());
    let tab = state.hits.tabs[0].0;
    let new_workspace = state.hits.new_workspace;

    state.handle_raw_events(vec![mouse(
        MouseEventKind::Down(MouseButton::Left),
        tab.x + 1,
        tab.y,
    )]);
    let drag = state.handle_raw_events(vec![mouse(
        MouseEventKind::Drag(MouseButton::Left),
        new_workspace.x,
        new_workspace.y,
    )]);
    assert!(drag.repaint);
    assert!(matches!(
        state.chrome_drag,
        Some(ClientChromeDrag::TabToWorkspace { ref target, .. })
            if *target == Some(TabWorkspaceDropTarget::NewWorkspace)
    ));
    // Rendering mid-drag must highlight the new-workspace drop target.
    let mid_drag = state.compose(106, 24).expect("frame mid-drag");
    let highlighted = cell_bg(&mid_drag, new_workspace.x, new_workspace.y);
    state.chrome_drag = None;
    let idle = state.compose(106, 24).expect("idle frame");
    assert_ne!(
        highlighted,
        cell_bg(&idle, new_workspace.x, new_workspace.y),
        "drop target should highlight mid-drag"
    );
    // Re-arm the drag after the idle render, then release.
    state.handle_raw_events(vec![mouse(
        MouseEventKind::Drag(MouseButton::Left),
        new_workspace.x,
        new_workspace.y,
    )]);
    assert!(matches!(
        state.chrome_drag,
        Some(ClientChromeDrag::TabToWorkspace { .. })
    ));

    let release = state.handle_raw_events(vec![mouse(
        MouseEventKind::Up(MouseButton::Left),
        new_workspace.x,
        new_workspace.y,
    )]);
    let methods = endpoint_methods(&release);
    // ws_1 holds a single pane, so the move empties it and the client
    // resurrects the source project with a fresh tab.
    assert_eq!(methods.len(), 2);
    let Method::PaneMove(params) = &methods[0] else {
        panic!("expected pane move, got {methods:?}");
    };
    assert_eq!(params.pane_id, "pane_1");
    assert!(params.focus);
    let PaneMoveDestination::NewWorkspace { label, tab_label } = &params.destination else {
        panic!("expected new workspace, got {:?}", params.destination);
    };
    assert_eq!(label.as_deref(), Some("alpha"));
    assert_eq!(tab_label.as_deref(), Some("alpha"));
    assert!(matches!(&methods[1], Method::WorkspaceCreate(_)));
}

#[test]
fn tab_drag_onto_existing_workspace_moves_into_it_without_focus() {
    let mut state = drag_state(two_workspace_snapshot());
    let tab = state.hits.tabs[0].0;
    let target_rect = state.hits.workspaces[1].rect;

    state.handle_raw_events(vec![mouse(
        MouseEventKind::Down(MouseButton::Left),
        tab.x + 1,
        tab.y,
    )]);
    state.handle_raw_events(vec![mouse(
        MouseEventKind::Drag(MouseButton::Left),
        target_rect.x + 1,
        target_rect.y,
    )]);
    assert!(matches!(
        state.chrome_drag,
        Some(ClientChromeDrag::TabToWorkspace { ref target, .. })
            if *target
                == Some(TabWorkspaceDropTarget::ExistingWorkspace {
                    workspace_id: "ws_2".into(),
                })
    ));
    // The hovered workspace row must highlight mid-drag.
    let mid_drag = state.compose(106, 24).expect("frame mid-drag");
    let highlighted = cell_bg(&mid_drag, target_rect.x + 1, target_rect.y);
    state.chrome_drag = None;
    let idle = state.compose(106, 24).expect("idle frame");
    assert_ne!(
        highlighted,
        cell_bg(&idle, target_rect.x + 1, target_rect.y),
        "hovered workspace should highlight mid-drag"
    );
    state.handle_raw_events(vec![mouse(
        MouseEventKind::Drag(MouseButton::Left),
        target_rect.x + 1,
        target_rect.y,
    )]);
    assert!(matches!(
        state.chrome_drag,
        Some(ClientChromeDrag::TabToWorkspace { .. })
    ));

    let release = state.handle_raw_events(vec![mouse(
        MouseEventKind::Up(MouseButton::Left),
        target_rect.x + 1,
        target_rect.y,
    )]);
    let methods = endpoint_methods(&release);
    // Single-pane source is resurrected after the move.
    assert_eq!(methods.len(), 2);
    let Method::PaneMove(params) = &methods[0] else {
        panic!("expected pane move, got {methods:?}");
    };
    assert!(!params.focus);
    let PaneMoveDestination::NewTab {
        workspace_id,
        label,
    } = &params.destination
    else {
        panic!("expected new tab, got {:?}", params.destination);
    };
    assert_eq!(workspace_id.as_deref(), Some("ws_2"));
    assert_eq!(label.as_deref(), Some("alpha"));
    assert!(matches!(&methods[1], Method::WorkspaceCreate(_)));
}

#[test]
fn tab_drag_onto_source_workspace_is_ignored() {
    let mut state = drag_state(two_workspace_snapshot());
    let tab = state.hits.tabs[0].0;
    let source_rect = state.hits.workspaces[0].rect;

    state.handle_raw_events(vec![mouse(
        MouseEventKind::Down(MouseButton::Left),
        tab.x + 1,
        tab.y,
    )]);
    state.handle_raw_events(vec![mouse(
        MouseEventKind::Drag(MouseButton::Left),
        source_rect.x + 1,
        source_rect.y,
    )]);
    let release = state.handle_raw_events(vec![mouse(
        MouseEventKind::Up(MouseButton::Left),
        source_rect.x + 1,
        source_rect.y,
    )]);
    // Releasing over the source project must not move anything; at most the
    // dragged tab itself is focused.
    for method in endpoint_methods(&release) {
        assert!(
            !matches!(
                method,
                Method::PaneMove(_)
                    | Method::TabMove(_)
                    | Method::WorkspaceCreate(_)
                    | Method::TabCreate(_)
            ),
            "unexpected move method: {method:?}"
        );
    }
    assert!(state.chrome_drag.is_none());
}

#[test]
fn moving_last_pane_resurrects_source_workspace() {
    let mut state = drag_state(snapshot());
    let tab = state.hits.tabs[0].0;
    let new_workspace = state.hits.new_workspace;

    state.handle_raw_events(vec![mouse(
        MouseEventKind::Down(MouseButton::Left),
        tab.x + 1,
        tab.y,
    )]);
    state.handle_raw_events(vec![mouse(
        MouseEventKind::Drag(MouseButton::Left),
        new_workspace.x,
        new_workspace.y,
    )]);
    let release = state.handle_raw_events(vec![mouse(
        MouseEventKind::Up(MouseButton::Left),
        new_workspace.x,
        new_workspace.y,
    )]);
    let methods = endpoint_methods(&release);
    assert_eq!(methods.len(), 2, "pane move plus workspace resurrect");
    assert!(matches!(&methods[0], Method::PaneMove(_)));
    let Method::WorkspaceCreate(params) = &methods[1] else {
        panic!("expected workspace resurrect, got {methods:?}");
    };
    assert!(!params.focus);
    // Default snapshot workspace uses an auto label, so the resurrect lets the
    // server rediscover it instead of freezing the old auto name.
    assert_eq!(params.label.as_deref(), None);
    assert_eq!(params.cwd.as_deref(), Some("/repo"));
}

#[test]
fn multi_pane_tab_drag_moves_only_focused_pane() {
    let mut projected = two_workspace_snapshot();
    let mut extra = projected.panes[0].clone();
    extra.pane_id = "pane_extra".into();
    extra.focused = false;
    projected.panes.push(extra);

    let mut state = drag_state(projected);
    let tab = state.hits.tabs[0].0;
    let new_workspace = state.hits.new_workspace;

    state.handle_raw_events(vec![mouse(
        MouseEventKind::Down(MouseButton::Left),
        tab.x + 1,
        tab.y,
    )]);
    state.handle_raw_events(vec![mouse(
        MouseEventKind::Drag(MouseButton::Left),
        new_workspace.x,
        new_workspace.y,
    )]);
    let release = state.handle_raw_events(vec![mouse(
        MouseEventKind::Up(MouseButton::Left),
        new_workspace.x,
        new_workspace.y,
    )]);
    let methods = endpoint_methods(&release);
    assert_eq!(methods.len(), 1, "source keeps its other pane");
    let Method::PaneMove(params) = &methods[0] else {
        panic!("expected pane move, got {methods:?}");
    };
    assert_eq!(params.pane_id, "pane_1");
}

#[test]
fn tab_context_menu_moves_active_pane_to_new_workspace() {
    let mut state = drag_state(two_workspace_snapshot());
    state.open_tab_context_menu("tab_1".into(), 30, 1);
    state.compose(106, 24).unwrap();
    let index = match state.overlay.as_ref() {
        Some(ClientShellOverlay::ContextMenu(menu)) => menu
            .items()
            .iter()
            .position(|item| item.action == ClientContextMenuAction::MoveToNewWorkspace)
            .expect("move to new workspace item"),
        _ => panic!("tab context menu"),
    };
    let row = state.hits.context_menu_rows[index].0;
    let outcome = state.handle_raw_events(vec![mouse(
        MouseEventKind::Down(MouseButton::Left),
        row.x + 1,
        row.y,
    )]);
    // Menu also focuses the tab first; the move itself must be present.
    let methods = endpoint_methods(&outcome);
    assert!(
        methods.iter().any(|method| matches!(
            method,
            Method::PaneMove(params) if params.pane_id == "pane_1"
                && matches!(
                    &params.destination,
                    PaneMoveDestination::NewWorkspace { .. }
                )
        )),
        "expected pane move in {methods:?}"
    );
}
