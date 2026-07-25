
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PhysicalPosition<T> {
    pub x: T,
    pub y: T,
}

impl<T> PhysicalPosition<T> {
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PhysicalSize<T> {
    pub width: T,
    pub height: T,
}

impl<T> PhysicalSize<T> {
    pub const fn new(width: T, height: T) -> Self {
        Self { width, height }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ElementState {
    Pressed,
    #[default]
    Released,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonSource {
    Mouse(MouseButton),
    Unknown(u32),
}

impl ButtonSource {
    pub const fn mouse_button(self) -> Option<MouseButton> {
        match self {
            Self::Mouse(button) => Some(button),
            Self::Unknown(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MouseScrollDelta {
    /// Continuous / finger-style delta in physical pixels (already scaled).
    PixelDelta(PhysicalPosition<f64>),
    /// Wheel-style logical steps from `axis_value120` (preferred) or discrete.
    /// Sign matches [`Self::PixelDelta`]: negate again in UI to get content delta.
    LineDelta { x: f64, y: f64 },
}

/// Touchpad pinch gesture phases for icon/view zoom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PinchGesture {
    Begin,
    /// Absolute scale relative to finger spacing at [`Self::Begin`].
    Update { scale: f64 },
    End { cancelled: bool },
}

/// Touchpad swipe gesture phases (history navigation when multi-finger).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SwipeGesture {
    Begin { fingers: u32 },
    /// Surface-coordinate delta since the previous update (logical coords).
    Update { delta_x: f64, delta_y: f64 },
    End { cancelled: bool },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Modifiers {
    control: bool,
    alt: bool,
    shift: bool,
    meta: bool,
    caps_lock: bool,
    num_lock: bool,
}

impl Modifiers {
    pub const fn state(self) -> Self {
        self
    }

    pub const fn control_key(self) -> bool {
        self.control
    }

    pub const fn alt_key(self) -> bool {
        self.alt
    }

    pub const fn shift_key(self) -> bool {
        self.shift
    }

    pub const fn meta_key(self) -> bool {
        self.meta
    }
}

impl From<wayland_client_runtime::Modifiers> for Modifiers {
    fn from(value: wayland_client_runtime::Modifiers) -> Self {
        Self {
            control: value.ctrl,
            alt: value.alt,
            shift: value.shift,
            meta: value.logo,
            caps_lock: value.caps_lock,
            num_lock: value.num_lock,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NamedKey {
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    Backspace,
    Delete,
    End,
    Enter,
    Escape,
    F1,
    F2,
    F3,
    F5,
    F6,
    Home,
    PageDown,
    PageUp,
    Tab,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum NativeKey {
    Unidentified,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Key {
    Named(NamedKey),
    Character(String),
    Unidentified(NativeKey),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KeyCode {
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    Backspace,
    Delete,
    Digit1,
    Digit2,
    Digit3,
    End,
    Escape,
    F1,
    F2,
    F3,
    F5,
    F6,
    Home,
    KeyA,
    KeyC,
    KeyD,
    KeyF,
    KeyH,
    KeyL,
    KeyR,
    KeyV,
    KeyX,
    Numpad1,
    Numpad2,
    Numpad3,
    Tab,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum NativeKeyCode {
    Unidentified,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PhysicalKey {
    Code(KeyCode),
    Unidentified(NativeKeyCode),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyEvent {
    pub physical_key: PhysicalKey,
    pub logical_key: Key,
    pub key_without_modifiers: Key,
    pub state: ElementState,
    pub repeat: bool,
    pub text: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CursorIcon {
    ColResize,
    #[default]
    Default,
    Pointer,
    Text,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Theme {
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DataTransferId(u64);

impl DataTransferId {
    pub const fn into_raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AsyncRequestSerial(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DndAction {
    Ask,
    Move,
    Copy,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum TypeHint {
    UriList,
    Plaintext,
}

impl TypeHint {
    pub fn matches(&self, other: &Self) -> bool {
        self == other
    }

    fn mime(&self) -> &'static str {
        match self {
            Self::UriList => "text/uri-list",
            Self::Plaintext => "text/plain;charset=utf-8",
        }
    }

    fn from_mime(mime: &str) -> Option<Self> {
        match mime {
            "text/uri-list" => Some(Self::UriList),
            "text/plain;charset=utf-8" | "text/plain" | "UTF8_STRING" => {
                Some(Self::Plaintext)
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DataType(Option<TypeHint>);

impl DataType {
    pub fn hint(&self) -> Option<TypeHint> {
        self.0.clone()
    }
}

pub trait TypedData: Any + Send + Sync {
    fn type_(&self) -> DataType;
    fn try_as_uris(&self) -> Result<Vec<String>, String>;
}

struct ReceivedTypedData {
    hint: TypeHint,
    result: Result<Vec<u8>, String>,
}

impl TypedData for ReceivedTypedData {
    fn type_(&self) -> DataType {
        DataType(Some(self.hint.clone()))
    }

    fn try_as_uris(&self) -> Result<Vec<String>, String> {
        if self.hint != TypeHint::UriList {
            return Err("received data is not a URI list".to_string());
        }
        let bytes = self.result.as_ref().map_err(Clone::clone)?;
        let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
        Ok(text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_string)
            .collect())
    }
}

#[derive(Clone, Debug)]
pub enum SendData {
    Uris(Vec<String>),
    Text(String),
}

impl From<String> for SendData {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

#[derive(Clone, Debug)]
pub struct DataTransferSend {
    payloads: Vec<(TypeHint, Arc<[u8]>)>,
}

pub struct DataTransferSendBuilder<T> {
    payload: T,
    payloads: Vec<(TypeHint, Arc<[u8]>)>,
}

impl<T> DataTransferSendBuilder<T> {
    pub fn new(payload: T) -> Self {
        Self {
            payload,
            payloads: Vec::new(),
        }
    }

    pub fn with_type<R, F>(mut self, hint: TypeHint, provider: F) -> Self
    where
        R: Into<SendData>,
        F: Fn(&T, &TypeHint) -> Option<R> + Send + Sync + 'static,
    {
        if let Some(data) = provider(&self.payload, &hint) {
            let bytes = match data.into() {
                SendData::Uris(uris) => {
                    let mut encoded = uris.join("\r\n");
                    if !encoded.is_empty() {
                        encoded.push_str("\r\n");
                    }
                    Arc::<[u8]>::from(encoded.into_bytes())
                }
                SendData::Text(text) => Arc::<[u8]>::from(text.into_bytes()),
            };
            self.payloads.push((hint, bytes));
        }
        self
    }

    pub fn build(self) -> DataTransferSend {
        let _ = self.payload;
        DataTransferSend {
            payloads: self.payloads,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RgbaIcon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl RgbaIcon {
    pub fn new(rgba: Vec<u8>, width: u32, height: u32) -> Result<Self, String> {
        if rgba.len() != width as usize * height as usize * 4 {
            return Err("RGBA icon byte length does not match its dimensions".to_string());
        }
        Ok(Self {
            rgba,
            width,
            height,
        })
    }
}

#[derive(Clone, Debug)]
pub struct DragIcon {
    pub icon: RgbaIcon,
    pub buffer_scale: i32,
    pub offset_x: i32,
    pub offset_y: i32,
}

#[derive(Clone, Debug)]
pub struct DataTransfer {
    hints: Vec<TypeHint>,
}

impl DataTransfer {
    pub fn has_type(&self, hint: &TypeHint) -> bool {
        self.hints.contains(hint)
    }
}

pub enum WindowEvent {
    CloseRequested,
    SurfaceResized(PhysicalSize<u32>),
    ScaleFactorChanged {
        scale_factor: f64,
    },
    ModifiersChanged(Modifiers),
    KeyboardInput {
        event: KeyEvent,
        is_synthetic: bool,
    },
    Ime(ImeEvent),
    PointerMoved {
        position: PhysicalPosition<f64>,
    },
    PointerLeft {},
    PointerButton {
        state: ElementState,
        position: PhysicalPosition<f64>,
        button: ButtonSource,
    },
    MouseWheel {
        delta: MouseScrollDelta,
    },
    /// Touchpad pinch from `zwp_pointer_gestures_v1` (main window only).
    ///
    /// `Update.scale` is absolute relative to finger spacing at [`PinchGesture::Begin`]
    /// (protocol scale, typically starting near 1.0).
    PinchGesture(PinchGesture),
    /// Touchpad swipe from `zwp_pointer_gestures_v1` (main window only).
    SwipeGesture(SwipeGesture),
    RedrawRequested,
    DragEntered {
        id: DataTransferId,
        position: Option<PhysicalPosition<f64>>,
    },
    DragPosition {
        id: DataTransferId,
        position: PhysicalPosition<f64>,
    },
    DragDropped {
        id: DataTransferId,
    },
    DragLeft {
        id: DataTransferId,
    },
    DataTransferReceived {
        id: DataTransferId,
        serial: AsyncRequestSerial,
        value: Arc<dyn TypedData>,
    },
    OutgoingDragDropped {
        id: DataTransferId,
        action: Option<DndAction>,
    },
    OutgoingDragCanceled {
        id: DataTransferId,
    },
}

pub type WindowId = SurfaceId;

#[derive(Clone, Copy, Debug, Default)]
pub enum ControlFlow {
    Poll,
    #[default]
    Wait,
    WaitUntil(Instant),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_list_payload_uses_crlf_and_a_terminal_line_break() {
        let transfer = DataTransferSendBuilder::new(())
            .with_type(TypeHint::UriList, |_, _| {
                Some(SendData::Uris(vec![
                    "file:///tmp/a".to_string(),
                    "file:///tmp/b".to_string(),
                ]))
            })
            .build();

        assert_eq!(transfer.payloads.len(), 1);
        assert_eq!(
            transfer.payloads[0].1.as_ref(),
            b"file:///tmp/a\r\nfile:///tmp/b\r\n"
        );
    }
}
