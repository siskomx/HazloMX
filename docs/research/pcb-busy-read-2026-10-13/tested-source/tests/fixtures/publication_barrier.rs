// Hold only the owned test artifact's first data read; attributes remain readable.
use std::ffi::c_void;
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    os::windows::{
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr,
    time::{Duration, Instant},
};

#[repr(C)]
struct Overlapped {
    internal: usize,
    internal_high: usize,
    offset: u32,
    offset_high: u32,
    event: *mut c_void,
}
#[link(name = "kernel32")]
extern "system" {
    fn CreateEventW(
        attributes: *const c_void,
        manual_reset: i32,
        initial_state: i32,
        name: *const u16,
    ) -> *mut c_void;
    fn DeviceIoControl(
        file: *mut c_void,
        code: u32,
        input: *const c_void,
        input_size: u32,
        output: *mut c_void,
        output_size: u32,
        returned: *mut u32,
        overlapped: *mut Overlapped,
    ) -> i32;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
}
pub(crate) struct Barrier {
    file: Option<File>,
    event: OwnedHandle,
}
impl Barrier {
    pub(crate) fn request(path: &Path) -> Self {
        assert_eq!(
            fs::symlink_metadata(path).unwrap().file_attributes() & 0x400,
            0
        );
        let file = OpenOptions::new()
            .read(true)
            .share_mode(7)
            .custom_flags(0x40000000)
            .open(path)
            .unwrap();
        let raw_event = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        assert!(!raw_event.is_null());
        let event = unsafe { OwnedHandle::from_raw_handle(raw_event) };
        // shortcut: one leaked OVERLAPPED per short-lived fixture process; add completion ownership if this becomes long-lived.
        let overlapped = Box::leak(Box::new(Overlapped {
            internal: 0,
            internal_high: 0,
            offset: 0,
            offset_high: 0,
            event: raw_event,
        }));
        let mut returned = 0;
        let accepted = unsafe {
            DeviceIoControl(
                file.as_raw_handle(),
                0x00090008,
                ptr::null(),
                0,
                ptr::null_mut(),
                0,
                &mut returned,
                overlapped,
            )
        };
        assert_eq!(accepted, 0, "Expected asynchronous batch oplock");
        assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(997));
        Self {
            file: Some(file),
            event,
        }
    }
    pub(crate) fn wait(&self, milliseconds: u32) -> u32 {
        unsafe { WaitForSingleObject(self.event.as_raw_handle(), milliseconds) }
    }
    pub(crate) fn release(&mut self) {
        drop(self.file.take());
    }
}
use std::os::windows::fs::MetadataExt;
fn record(state: &str) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    fs::write(
        format!(r"C:\HazloProbe\output\publication-barrier-{state}.json"),
        format!(
            r#"{{"state":"{state}","pid":{},"unix_ms":{now}}}"#,
            std::process::id()
        ),
    )
    .unwrap();
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--self-check") {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let root = std::env::temp_dir().join(format!(
            "hazlomexa-oplock-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("solid.FCStd");
        fs::write(&path, b"preserve artifact bytes").unwrap();
        let mut barrier = Barrier::request(&path);
        fs::symlink_metadata(&path).unwrap();
        assert_eq!(
            barrier.wait(100),
            258,
            "Attributes unexpectedly broke the oplock"
        );
        let opened = Arc::new(AtomicBool::new(false));
        let observed = opened.clone();
        let reader_path = path.clone();
        let reader = std::thread::spawn(move || {
            let mut file = OpenOptions::new()
                .read(true)
                .share_mode(1)
                .custom_flags(0x00200000)
                .open(reader_path)
                .unwrap();
            observed.store(true, Ordering::SeqCst);
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            bytes
        });
        assert_eq!(barrier.wait(5000), 0, "Data read did not break the oplock");
        assert!(
            !opened.load(Ordering::SeqCst),
            "Reader was not held before acknowledgment"
        );
        barrier.release();
        assert_eq!(reader.join().unwrap(), b"preserve artifact bytes");
        fs::remove_file(&path).unwrap();
        fs::remove_dir(&root).unwrap();
        println!("PASS: attributes do not break the oplock; data-read open is held until release; bytes preserved");
        return;
    }
    assert_eq!(std::env::var("USERNAME").unwrap(), "WDAGUtilityAccount");
    assert_eq!(args.len(), 3);
    assert_eq!(args[1], "--stage");
    let stage = Path::new(&args[2]);
    let job = stage
        .strip_prefix(r"C:\HazloGuestProject\.hazlomexa\staging")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(job.len(), 36);
    for (index, byte) in job.bytes().enumerate() {
        assert!(if [8, 13, 18, 23].contains(&index) {
            byte == b'-'
        } else {
            byte.is_ascii_hexdigit()
        });
    }
    let release = Path::new(r"C:\HazloProbe\release-publication-barrier.txt");
    assert_eq!(fs::read_to_string(release).unwrap().trim(), "hold");
    let deadline = Instant::now() + Duration::from_secs(90);
    while !stage.join("solid.step").exists() {
        assert!(Instant::now() < deadline, "STEP export did not start");
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut barrier = Barrier::request(&stage.join("solid.FCStd"));
    record("granted");
    assert_eq!(
        barrier.wait(90000),
        0,
        "Publication data read did not arrive"
    );
    record("broken");
    let deadline = Instant::now() + Duration::from_secs(90);
    while fs::read_to_string(release).unwrap().trim() != "release" {
        assert!(
            Instant::now() < deadline,
            "Barrier release deadline elapsed"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    barrier.release();
    record("released");
}
