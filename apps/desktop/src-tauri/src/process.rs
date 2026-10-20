//! Owned Windows processes; this module does not provide filesystem isolation.
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::File,
    io::{self, Read, Write},
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, IntoRawHandle, OwnedHandle},
    },
    path::Path,
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, WAIT_OBJECT_0, WAIT_TIMEOUT},
    Security::SECURITY_ATTRIBUTES,
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            JobObjectBasicAccountingInformation, JobObjectBasicProcessIdList, QueryInformationJobObject, IsProcessInJob,
            SetInformationJobObject, TerminateJobObject, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOBOBJECT_BASIC_PROCESS_ID_LIST,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
        Pipes::CreatePipe,
        Threading::{
            CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess, OpenProcess,
            InitializeProcThreadAttributeList, ResumeThread, TerminateProcess,
            UpdateProcThreadAttribute, WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED,
            CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, PROCESS_INFORMATION,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST, STARTF_USESTDHANDLES, STARTUPINFOEXW, PROCESS_SYNCHRONIZE, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
};

const LOG_LIMIT: usize = 256 * 1024;

#[derive(Clone, Copy)]
pub(crate) struct DispatchGate<'a> { pub mutex: &'a Mutex<()>, pub paused: &'a AtomicBool }
impl<'a> DispatchGate<'a> {
    pub(crate) fn enter(self, cancelled: &AtomicBool) -> Result<MutexGuard<'a, ()>, String> {
        let permit=self.mutex.lock().map_err(|_|"No se pudo comprobar la pausa de despacho.")?;
        if cancelled.load(Ordering::SeqCst) || self.paused.load(Ordering::SeqCst) {
            return Err("Los pasos pendientes están pausados. Se conserva el trabajo; no se envían más llamadas.".into());
        }
        Ok(permit)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum StopReason {
    Completed,
    Cancelled,
    TimedOut,
}

#[derive(Debug)]
pub struct ProcessOutput {
    pub reason: StopReason,
    pub exit_code: Option<u32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub truncated: bool,
    // Retain stopped identities until the result is consumed; PIDs can otherwise be reused.
    _stopped: Vec<OwnedHandle>,
}

fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut result: Vec<_> = value.encode_wide().collect();
    if result.contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Embedded NUL"));
    }
    result.push(0);
    Ok(result)
}

// Windows CRT argv quoting; application identity is separately passed to CreateProcessW.
fn quote(value: &OsStr) -> io::Result<Vec<u16>> {
    let value = wide(value)?;
    let mut result = vec![b'"' as u16];
    let mut slashes = 0;
    for &unit in &value[..value.len() - 1] {
        if unit == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        result.extend(std::iter::repeat_n(
            b'\\' as u16,
            if unit == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        result.push(unit);
        slashes = 0;
    }
    result.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    result.push(b'"' as u16);
    Ok(result)
}

fn environment(values: &BTreeMap<OsString, OsString>) -> io::Result<Vec<u16>> {
    let mut pairs: Vec<_> = values.iter().collect();
    pairs.sort_by_key(|(key, _)| key.to_string_lossy().to_uppercase());
    let mut result = Vec::new();
    for (key, value) in pairs {
        if key.is_empty() || key.to_string_lossy().contains('=') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid environment key",
            ));
        }
        let mut key = wide(key)?;
        key.pop();
        result.extend(key);
        result.push(b'=' as u16);
        result.extend(wide(value)?);
    }
    if result.is_empty() {
        result.push(0);
    }
    result.push(0);
    Ok(result)
}

fn handle(raw: HANDLE) -> io::Result<OwnedHandle> {
    if raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful native creation transfers one valid handle to this owner.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}

fn pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    let (mut read, mut write) = (null_mut(), null_mut());
    if unsafe { CreatePipe(&mut read, &mut write, &security, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((handle(read)?, handle(write)?))
}

struct Attributes {
    buffer: Vec<usize>,
}
impl Attributes {
    fn pointer(&mut self) -> *mut std::ffi::c_void {
        self.buffer.as_mut_ptr().cast()
    }
    fn new(handles: &[HANDLE], console:Option<isize>) -> io::Result<Self> {
        let mut bytes = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut bytes);
        }
        let mut buffer = vec![0usize; bytes.div_ceil(size_of::<usize>())];
        if unsafe {
            InitializeProcThreadAttributeList(buffer.as_mut_ptr().cast(), 1, 0, &mut bytes)
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut result = Self { buffer };
        if unsafe {
            UpdateProcThreadAttribute(
                result.pointer().cast(),
                0,
                if console.is_some(){windows_sys::Win32::System::Threading::PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize}else{PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize},
                console.map_or(handles.as_ptr().cast_mut().cast(),|handle|handle as *mut _),
                if console.is_some(){size_of::<isize>()}else{std::mem::size_of_val(handles)},
                null_mut(),
                null(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(result)
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.pointer().cast());
        }
    }
}

struct Job(OwnedHandle);
struct Console(isize);
impl Drop for Console {
    fn drop(&mut self){unsafe {windows_sys::Win32::System::Console::ClosePseudoConsole(self.0);}}
}
impl Job {
    fn new() -> io::Result<Self> {
        let job = Self(handle(unsafe { CreateJobObjectW(null(), null()) })?);
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                job.0.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }
    fn members(&self,known:&mut BTreeMap<usize,OwnedHandle>)->io::Result<()> {
        const CAPACITY:usize=1024;
        let header=size_of::<JOBOBJECT_BASIC_PROCESS_ID_LIST>()/size_of::<usize>()-1;
        let mut buffer=vec![0usize;header+CAPACITY];let list=buffer.as_mut_ptr().cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>();
        if unsafe{QueryInformationJobObject(self.0.as_raw_handle(),JobObjectBasicProcessIdList,list.cast(),std::mem::size_of_val(buffer.as_slice()) as u32,null_mut())}==0{return Err(io::Error::last_os_error());}
        let count=unsafe{(*list).NumberOfProcessIdsInList} as usize;
        if count>CAPACITY || unsafe{(*list).NumberOfAssignedProcesses} as usize!=count{return Err(io::Error::other("Owned job member limit exceeded"));}
        // The native count is bounded by the allocated variable-length array above.
        let ids=unsafe{std::slice::from_raw_parts(std::ptr::addr_of!((*list).ProcessIdList).cast::<usize>(),count)};
        for &pid in ids{
            if known.contains_key(&pid){continue;}
            let raw=unsafe{OpenProcess(PROCESS_SYNCHRONIZE|PROCESS_QUERY_LIMITED_INFORMATION,0,pid.try_into().map_err(|_|io::Error::other("Invalid owned process identity"))?)};
            if raw.is_null(){let error=io::Error::last_os_error();if error.raw_os_error()==Some(windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER as i32){continue;}return Err(error);}
            let process=handle(raw)?;let mut member=0;
            if unsafe{IsProcessInJob(process.as_raw_handle(),self.0.as_raw_handle(),&mut member)}==0{return Err(io::Error::last_os_error());}
            if member!=0{known.insert(pid,process);}
        }
        Ok(())
    }
    fn stop(&self) -> io::Result<Vec<OwnedHandle>> {
        let mut members=BTreeMap::new();self.members(&mut members)?;
        if unsafe { TerminateJobObject(self.0.as_raw_handle(), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // Counts can clear during teardown; also wait on the original process identities.
        let started = Instant::now();
        loop {
            self.members(&mut members)?;
            let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
            if unsafe {
                QueryInformationJobObject(self.0.as_raw_handle(), JobObjectBasicAccountingInformation,
                    (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                    size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32, null_mut())
            } == 0 { return Err(io::Error::last_os_error()); }
            let stopped=members.values().all(|process|unsafe{WaitForSingleObject(process.as_raw_handle(),0)}==WAIT_OBJECT_0);
            if accounting.ActiveProcesses == 0 && stopped { return Ok(members.into_values().collect()); }
            if started.elapsed() >= Duration::from_secs(5) {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "Owned process tree did not stop"));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn drain(pipe: OwnedHandle) -> io::Result<(Vec<u8>, bool)> {
    // SAFETY: consuming the owned handle transfers it to File exactly once.
    let mut file = unsafe { File::from_raw_handle(pipe.into_raw_handle()) };
    let mut output = Vec::new();
    let mut truncated = false;
    let mut chunk = [0; 8192];
    loop {
        let count = file.read(&mut chunk)?;
        if count == 0 {
            return Ok((output, truncated));
        }
        let keep = count.min(LOG_LIMIT - output.len());
        output.extend_from_slice(&chunk[..keep]);
        truncated |= keep < count;
    }
}

/// Caller supplies an identified executable, typed arguments and an explicit environment.
/// Cancellation terminates only this job; it is not rollback or an OS sandbox.
pub fn run_owned(
    executable: &Path,
    arguments: &[OsString],
    cwd: &Path,
    variables: &BTreeMap<OsString, OsString>,
    cancelled: Arc<AtomicBool>,
    timeout: Duration,
) -> io::Result<ProcessOutput> {
    run_owned_input(executable, arguments, cwd, variables, cancelled, timeout, &[])
}

/// Private stdin stays off command lines and disk; cancellation also stops a blocked writer.
pub fn run_owned_input(
    executable: &Path,
    arguments: &[OsString],
    cwd: &Path,
    variables: &BTreeMap<OsString, OsString>,
    cancelled: Arc<AtomicBool>,
    timeout: Duration,
    input: &[u8],
) -> io::Result<ProcessOutput> {
    run_owned_streams(executable,arguments,cwd,variables,cancelled,timeout,(input,false,None))
}

/// A hidden native console also captures programs that reopen CONOUT$ instead of using their pipes.
pub fn run_owned_console(executable:&Path,arguments:&[OsString],cwd:&Path,variables:&BTreeMap<OsString,OsString>,cancelled:Arc<AtomicBool>,timeout:Duration)->io::Result<ProcessOutput> {
    run_owned_streams(executable,arguments,cwd,variables,cancelled,timeout,(&[],true,None))
}

pub(crate) fn run_owned_permitted(executable:&Path,arguments:&[OsString],cwd:&Path,variables:&BTreeMap<OsString,OsString>,cancelled:Arc<AtomicBool>,timeout:Duration,mode:(bool,Option<MutexGuard<'_,()>>))->io::Result<ProcessOutput> {
    run_owned_streams(executable,arguments,cwd,variables,cancelled,timeout,(&[],mode.0,mode.1))
}
pub(crate) fn run_owned_input_permitted(executable:&Path,arguments:&[OsString],cwd:&Path,variables:&BTreeMap<OsString,OsString>,cancelled:Arc<AtomicBool>,timeout:Duration,mode:(&[u8],MutexGuard<'_,()>))->io::Result<ProcessOutput>{
    run_owned_streams(executable,arguments,cwd,variables,cancelled,timeout,(mode.0,false,Some(mode.1)))
}
fn run_owned_streams(executable:&Path,arguments:&[OsString],cwd:&Path,variables:&BTreeMap<OsString,OsString>,cancelled:Arc<AtomicBool>,timeout:Duration,mode:(&[u8],bool,Option<MutexGuard<'_,()>>))->io::Result<ProcessOutput> {
    let (input,console_mode,permit)=mode;
    if input.len() > LOG_LIMIT {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Stdin exceeds limit"));
    }
    if cancelled.load(Ordering::SeqCst) {
        return Ok(ProcessOutput {
            _stopped: Vec::new(),
            reason: StopReason::Cancelled,
            exit_code: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            truncated: false,
        });
    }
    if !executable.is_absolute() || !cwd.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Executable and working directory must be absolute",
        ));
    }
    let executable_name = wide(executable.as_os_str())?;
    let directory = wide(cwd.as_os_str())?;
    let mut command = quote(executable.as_os_str())?;
    for argument in arguments {
        command.push(b' ' as u16);
        command.extend(quote(argument)?);
    }
    command.push(0);
    if command.len() > 32767 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Command line too long",
        ));
    }
    let mut variables = environment(variables)?;
    let job = Job::new()?;
    let (stdin_read, stdin_write) = pipe()?;
    let (stdout_read, stdout_write) = pipe()?;
    let (stderr_read, stderr_write) = pipe()?;
    for read in [&stdout_read, &stderr_read] {
        if unsafe { SetHandleInformation(read.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    let inherited = [
        stdin_read.as_raw_handle(),
        stdout_write.as_raw_handle(),
        stderr_write.as_raw_handle(),
    ];
    let console=if console_mode {
        let mut handle=0;
        let result=unsafe {windows_sys::Win32::System::Console::CreatePseudoConsole(windows_sys::Win32::System::Console::COORD{X:240,Y:80},stdin_read.as_raw_handle(),stdout_write.as_raw_handle(),0,&mut handle)};
        if result<0 {return Err(io::Error::other(format!("Native console creation failed: {result:#x}")));}
        Some(Console(handle))
    }else{None};
    let mut attributes = Attributes::new(&inherited,console.as_ref().map(|c|c.0))?;
    let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    if !console_mode {
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = inherited[0];
        startup.StartupInfo.hStdOutput = inherited[1];
        startup.StartupInfo.hStdError = inherited[2];
    }
    startup.lpAttributeList = attributes.pointer().cast();
    let mut information: PROCESS_INFORMATION = unsafe { zeroed() };
    if unsafe {
        CreateProcessW(
            executable_name.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            if console_mode{0}else{1},
            CREATE_SUSPENDED
                | (if console_mode{0}else{CREATE_NO_WINDOW})
                | CREATE_UNICODE_ENVIRONMENT
                | EXTENDED_STARTUPINFO_PRESENT,
            variables.as_mut_ptr().cast(),
            directory.as_ptr(),
            &startup.StartupInfo,
            &mut information,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let process = handle(information.hProcess)?;
    let primary_thread = handle(information.hThread)?;
    if unsafe { AssignProcessToJobObject(job.0.as_raw_handle(), process.as_raw_handle()) } == 0 {
        let error = io::Error::last_os_error();
        unsafe {
            TerminateProcess(process.as_raw_handle(), 1);
            WaitForSingleObject(process.as_raw_handle(), 5000);
        }
        return Err(error);
    }
    drop(stdin_read);
    drop(stdout_write);
    drop(stderr_write);
    let stdout = thread::Builder::new().spawn(move || drain(stdout_read))?;
    let stderr = thread::Builder::new().spawn(move || drain(stderr_read))?;
    let input = input.to_vec();
    let (stdin,console_input)=if console_mode {(None,Some(stdin_write))}else{(Some(thread::Builder::new().spawn(move || {
        // SAFETY: the writer owns the transferred handle; closing it sends EOF.
        let mut file = unsafe { File::from_raw_handle(stdin_write.into_raw_handle()) };
        file.write_all(&input)
    })?),None)};
    if !cancelled.load(Ordering::SeqCst)
        && unsafe { ResumeThread(primary_thread.as_raw_handle()) } == u32::MAX
    {
        return Err(io::Error::last_os_error());
    }
    drop(permit);
    let started = Instant::now();
    let reason = loop {
        if cancelled.load(Ordering::SeqCst) {
            break StopReason::Cancelled;
        }
        if started.elapsed() >= timeout {
            break StopReason::TimedOut;
        }
        match unsafe { WaitForSingleObject(process.as_raw_handle(), 25) } {
            WAIT_OBJECT_0 => break StopReason::Completed,
            WAIT_TIMEOUT => (),
            _ => return Err(io::Error::last_os_error()),
        }
    };
    // A completed parent can still leave descendants holding our log pipes open.
    let stopped=job.stop()?;
    if unsafe { WaitForSingleObject(process.as_raw_handle(), 5000) } != WAIT_OBJECT_0 {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Owned process did not stop",
        ));
    }
    let mut exit_code = 0;
    if unsafe { GetExitCodeProcess(process.as_raw_handle(), &mut exit_code) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // Output drains on its own thread while the virtual console flushes and closes.
    drop(console_input);
    drop(console);
    if let Some(stdin)=stdin {
        let written = stdin.join().map_err(|_| io::Error::other("Stdin writer panicked"))?;
        if reason == StopReason::Completed {written?;}
    }
    let (stdout, stdout_truncated) = stdout
        .join()
        .map_err(|_| io::Error::other("Stdout reader panicked"))??;
    let (stderr, stderr_truncated) = stderr
        .join()
        .map_err(|_| io::Error::other("Stderr reader panicked"))??;
    Ok(ProcessOutput {
        _stopped: stopped,
        reason,
        exit_code: Some(exit_code),
        stdout,
        stderr,
        truncated: stdout_truncated || stderr_truncated,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::{fs, process::Command, sync::OnceLock};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE};

    #[test]
    fn sdk_stdio_close_stops_owned_tree() {
        use process_wrap::tokio::{CommandWrap, CreationFlags, JobObject, KillOnDrop};
        use rmcp::{
            transport::{TokioChildProcess, Transport},
            RoleClient,
        };
        let root =
            std::env::temp_dir().join(format!("hazlomexa-sdk-tree-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut command = tokio::process::Command::new(probe());
            command.arg("tree").current_dir(&root).kill_on_drop(true);
            let mut wrapped = CommandWrap::from(command);
            let mut flags = CreationFlags(Default::default());
            flags.0 .0 = 0x08000000;
            wrapped.wrap(flags).wrap(KillOnDrop).wrap(JobObject);
            let mut transport = TokioChildProcess::new(wrapped).unwrap();
            let parent = transport.id().unwrap();
            let start = Instant::now();
            while !root.join("leaf.pid").exists() {
                assert!(start.elapsed() < Duration::from_secs(10));
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let child: u32 = fs::read_to_string(root.join("child.pid"))
                .unwrap()
                .parse()
                .unwrap();
            assert!(is_alive(parent));
            assert!(is_alive(child));
            <TokioChildProcess as Transport<RoleClient>>::close(&mut transport)
                .await
                .unwrap();
            let cessation = Instant::now();
            while is_alive(child) {
                assert!(
                    cessation.elapsed() < Duration::from_secs(5),
                    "Owned descendant did not cease after SDK close"
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert!(!is_alive(parent));
            assert!(!is_alive(child));
        });
        fs::remove_dir_all(root).unwrap();
    }

    pub(crate) fn probe() -> &'static Path {
        static PROBE: OnceLock<std::path::PathBuf> = OnceLock::new();
        PROBE.get_or_init(|| {
            if let Some(path) = std::env::var_os("HAZLOMEXA_PROCESS_PROBE_EXE") {
                let executable = std::path::PathBuf::from(path).canonicalize().unwrap();
                assert!(executable.is_file(), "Prebuilt test helper is missing");
                return executable;
            }
            let output =
                std::env::temp_dir().join(format!("hazlomexa-probe-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&output).unwrap();
            let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../tests/fixtures/process_probe.rs");
            let executable = output.join("probe.exe");
            let result = Command::new("rustc")
                .arg(source)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            executable
        })
    }

    fn variables() -> BTreeMap<OsString, OsString> {
        ["SystemRoot", "WINDIR", "TEMP", "TMP"]
            .into_iter()
            .filter_map(|key| std::env::var_os(key).map(|value| (OsString::from(key), value)))
            .collect()
    }

    fn workspace() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("hazlomexa-process-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        path
    }

    pub(crate) fn child_processes(name: &str) -> Vec<u32> {
        child_processes_of(std::process::id(), name)
    }

    pub(crate) fn child_processes_of(parent: u32, name: &str) -> Vec<u32> {
        use windows_sys::Win32::{
            Foundation::INVALID_HANDLE_VALUE,
            System::Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
        };
        let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        assert_ne!(raw, INVALID_HANDLE_VALUE);
        let snapshot = handle(raw).unwrap();
        let mut entry: PROCESSENTRY32W = unsafe { zeroed() };
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut found = Vec::new();
        let mut ready = unsafe { Process32FirstW(snapshot.as_raw_handle(), &mut entry) };
        while ready != 0 {
            let end = entry.szExeFile.iter().position(|v| *v == 0).unwrap();
            if entry.th32ParentProcessID == parent
                && String::from_utf16_lossy(&entry.szExeFile[..end]).eq_ignore_ascii_case(name)
            {
                found.push(entry.th32ProcessID);
            }
            ready = unsafe { Process32NextW(snapshot.as_raw_handle(), &mut entry) };
        }
        found
    }

    pub(crate) fn is_alive(pid: u32) -> bool {
        let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if raw.is_null() {
            return false;
        }
        let owned = handle(raw).unwrap();
        unsafe { WaitForSingleObject(owned.as_raw_handle(), 0) == WAIT_TIMEOUT }
    }

    #[test]
    fn quotes_arguments_and_drains_both_pipes_under_limits() {
        let root = workspace();
        let arguments: Vec<_> = ["noisy", "á's spaces", "quote\"inside", "ends\\", ""]
            .map(OsString::from)
            .into();
        let output = run_owned(
            probe(),
            &arguments,
            &root,
            &variables(),
            Arc::new(AtomicBool::new(false)),
            Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(output.reason, StopReason::Completed);
        assert_eq!(output.exit_code, Some(0));
        assert!(output.truncated);
        assert_eq!(output.stdout.len(), LOG_LIMIT);
        assert_eq!(output.stderr.len(), LOG_LIMIT);
        let actual: Vec<String> =
            serde_json::from_slice(&fs::read(root.join("arguments.json")).unwrap()).unwrap();
        assert_eq!(actual, ["á's spaces", "quote\"inside", "ends\\", ""]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn private_input_roundtrips_and_eof_is_delivered() {
        let root = workspace();
        let input = vec![b'x'; LOG_LIMIT];
        let result = run_owned_input(
            probe(), &["input".into()], &root, &variables(),
            Arc::new(AtomicBool::new(false)), Duration::from_secs(10), &input,
        ).unwrap();
        assert_eq!(result.reason, StopReason::Completed);
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout, input);
        assert!(!result.truncated);
        assert!(result.stderr.is_empty());
        assert!(run_owned_input(
            probe(), &["input".into()], &root, &variables(),
            Arc::new(AtomicBool::new(false)), Duration::from_secs(10),
            &vec![b'x'; LOG_LIMIT + 1],
        ).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancellation_releases_blocked_stdin_writer_and_owned_tree() {
        let root = workspace();
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let signal_root = root.clone();
        let setter = thread::spawn(move || {
            let started = Instant::now();
            while !signal_root.join("child.pid").exists() && started.elapsed() < Duration::from_secs(5) {
                thread::sleep(Duration::from_millis(10));
            }
            signal.store(true, Ordering::SeqCst);
        });
        let started = Instant::now();
        let output = run_owned_input(
            probe(), &["tree".into()], &root, &variables(), cancel,
            Duration::from_secs(10), &vec![b'x'; LOG_LIMIT],
        ).unwrap();
        setter.join().unwrap();
        assert_eq!(output.reason, StopReason::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(5));
        let child: u32 = fs::read_to_string(root.join("child.pid")).unwrap().parse().unwrap();
        assert!(!is_alive(child));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cli_dispatch_permit_is_released_while_the_owned_process_is_running() {
        let root=workspace();let gate=Arc::new(std::sync::Mutex::new(()));
        let cancelled=Arc::new(AtomicBool::new(false));
        let worker_gate=gate.clone();let worker_cancel=cancelled.clone();let worker_root=root.clone();
        let worker=thread::spawn(move||{
            let permit=worker_gate.lock().unwrap();
            run_owned_permitted(probe(),&["tree".into()],&worker_root,&variables(),worker_cancel,Duration::from_secs(10),(false,Some(permit))).unwrap()
        });
        let started=Instant::now();
        while !root.join("child.pid").exists() && started.elapsed()<Duration::from_secs(5){thread::sleep(Duration::from_millis(10));}
        assert!(root.join("child.pid").exists());
        let permit=gate.try_lock().expect("Correction must not wait for CLI completion");
        cancelled.store(true,Ordering::SeqCst);drop(permit);
        assert_eq!(worker.join().unwrap().reason,StopReason::Cancelled);
        let child:u32=fs::read_to_string(root.join("child.pid")).unwrap().parse().unwrap();
        // Termination is asynchronous; observe the owned child instead of assuming immediate exit.
        let stopped=Instant::now();while is_alive(child)&&stopped.elapsed()<Duration::from_secs(5){thread::sleep(Duration::from_millis(10));}
        assert!(!is_alive(child));
        assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn parent_completion_waits_for_descendants_without_shared_log_pipes() {
        let root=workspace();
        for _ in 0..16{
            let output=run_owned(probe(),&["tree-exit".into()],&root,&variables(),Arc::new(AtomicBool::new(false)),Duration::from_secs(10)).unwrap();
            assert_eq!(output.reason,StopReason::Completed);
            let child:u32=fs::read_to_string(root.join("child.pid")).unwrap().parse().unwrap();
            assert!(!is_alive(child));
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stdin_worker_dispatch_permit_releases_before_waiting_for_the_owned_process() {
        let root=workspace();let gate=Arc::new(std::sync::Mutex::new(()));let cancel=Arc::new(AtomicBool::new(false));
        let worker_root=root.clone();let worker_gate=gate.clone();let worker_cancel=cancel.clone();
        let worker=thread::spawn(move||{let permit=worker_gate.lock().unwrap();run_owned_input_permitted(probe(),&["tree".into()],&worker_root,&variables(),worker_cancel,Duration::from_secs(10),(b"PRIVATE_STDIN_SENTINEL",permit)).unwrap()});
        let started=Instant::now();while !root.join("child.pid").exists()&&started.elapsed()<Duration::from_secs(5){thread::sleep(Duration::from_millis(10));}
        assert!(root.join("child.pid").exists());let permit=gate.try_lock().expect("Correction must not wait for the stdin worker");cancel.store(true,Ordering::SeqCst);drop(permit);
        assert_eq!(worker.join().unwrap().reason,StopReason::Cancelled);let child:u32=fs::read_to_string(root.join("child.pid")).unwrap().parse().unwrap();assert!(!is_alive(child));
        assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancellation_stops_owned_descendants_but_preserves_unrelated_process() {
        let root = workspace();
        let unrelated_root = workspace();
        let mut unrelated = Command::new(probe())
            .arg("leaf")
            .current_dir(&unrelated_root)
            .spawn()
            .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let signal_root = root.clone();
        let setter = thread::spawn(move || {
            let started = Instant::now();
            while !signal_root.join("child.pid").exists()
                && started.elapsed() < Duration::from_secs(5)
            {
                thread::sleep(Duration::from_millis(10));
            }
            signal.store(true, Ordering::SeqCst);
        });
        let started = Instant::now();
        let result = run_owned(
            probe(),
            &["tree".into()],
            &root,
            &variables(),
            cancel,
            Duration::from_secs(10),
        );
        setter.join().unwrap();
        let survived = unrelated.try_wait().unwrap().is_none();
        unrelated.kill().unwrap();
        unrelated.wait().unwrap();
        let output = result.unwrap();
        assert_eq!(output.reason, StopReason::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(5));
        let child: u32 = fs::read_to_string(root.join("child.pid"))
            .unwrap()
            .parse()
            .unwrap();
        assert!(!is_alive(child));
        assert!(survived);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(unrelated_root).unwrap();
    }

    #[test]
    fn cancellation_before_dispatch_creates_no_process_or_file() {
        let root = workspace();
        let output = run_owned(
            probe(),
            &["leaf".into()],
            &root,
            &variables(),
            Arc::new(AtomicBool::new(true)),
            Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(output.reason, StopReason::Cancelled);
        assert_eq!(output.exit_code, None);
        assert!(fs::read_dir(&root).unwrap().next().is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn timeout_is_distinct_from_completion() {
        let root = workspace();
        let started = Instant::now();
        let output = run_owned(
            probe(),
            &["leaf".into()],
            &root,
            &variables(),
            Arc::new(AtomicBool::new(false)),
            Duration::from_millis(100),
        )
        .unwrap();
        assert_eq!(output.reason, StopReason::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(5));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn abrupt_owner_exit_closes_job_and_terminates_descendants() {
        let root = workspace();
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "process::tests::crash_owner_helper"])
            .env("HAZLOMEXA_PROBE_ROOT", &root)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(19),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let child: u32 = fs::read_to_string(root.join("child.pid"))
            .unwrap()
            .parse()
            .unwrap();
        let started = Instant::now();
        while is_alive(child) && started.elapsed() < Duration::from_secs(5) {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!is_alive(child));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "subprocess helper for the abrupt-owner-exit test"]
    fn crash_owner_helper() {
        let root = std::path::PathBuf::from(std::env::var_os("HAZLOMEXA_PROBE_ROOT").unwrap());
        let worker_root = root.clone();
        thread::spawn(move || {
            let started = Instant::now();
            while !worker_root.join("child.pid").exists()
                && started.elapsed() < Duration::from_secs(5)
            {
                thread::sleep(Duration::from_millis(10));
            }
            // ExitProcess bypasses Rust destructors: only Windows handle closure can clean up.
            unsafe {
                windows_sys::Win32::System::Threading::ExitProcess(19);
            }
        });
        run_owned(
            probe(),
            &["tree".into()],
            &root,
            &variables(),
            Arc::new(AtomicBool::new(false)),
            Duration::from_secs(10),
        )
        .unwrap();
        panic!("Owner exited too late");
    }
}
