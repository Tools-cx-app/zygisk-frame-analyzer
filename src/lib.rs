use jni::JNIEnv;
use jni_sys::{JNINativeInterface_, JNINativeMethod, jbyteArray, jint, jlong};
use std::ffi::{CString, c_void};
use std::fs;
use std::sync::{LazyLock, Mutex};
use zygisk_rs::{Api, Module, register_zygisk_module};

static ORIG: LazyLock<Mutex<Option<extern "C" fn(jlong, jbyteArray, jint) -> jint>>> =
    LazyLock::new(|| Mutex::new(None));

extern "C" fn nsync(
    env: *mut jni_sys::JNIEnv,
    _jclass: jni_sys::jclass,
    native_ptr: jlong,
    frame_info: jbyteArray,
    sync_mode: jint,
) -> jint {
    unsafe {
        let func = (**env).v1_6;

        let elements = (func.GetByteArrayElements)(env, frame_info, std::ptr::null_mut());
        if elements.is_null() {
            return -1;
        }

        let vsync_ns = *(elements as *const i64);
        let vsync_ms = vsync_ns / 1_000_000;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        log::info!("[hook] vsync={} ms  now={} ms", vsync_ms, now_ms);

        (func.ReleaseByteArrayElements)(env, frame_info, elements, jni_sys::JNI_ABORT);
    }

    let orig = ORIG.lock().unwrap();
    if let Some(f) = *orig {
        drop(orig);
        f(native_ptr, frame_info, sync_mode)
    } else {
        -1
    }
}

struct Zygisk {
    api: Api,
    env: JNIEnv<'static>,
}

impl Module for Zygisk {
    fn new(api: Api, env: *mut jni_sys::JNIEnv) -> Self {
        #[cfg(any(target_os = "android"))]
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Info) // 使用 Trace 级别
                .with_tag("zygisk-frame-analyzer"),
        );

        let env = unsafe { JNIEnv::from_raw(env.cast()).unwrap() };

        Self { api, env }
    }

    fn pre_app_specialize(&mut self, args: &mut zygisk_rs::AppSpecializeArgs) {
        // get app pids
        let package_name = self
            .env
            .get_string(unsafe {
                (args.nice_name as *mut jni_sys::jstring as *mut ()
                    as *const jni::objects::JString<'_>)
                    .as_ref()
                    .unwrap()
            })
            .unwrap()
            .to_string_lossy()
            .to_string();
        let mut pids = vec![];

        for i in fs::read_dir("/proc/").unwrap() {
            let dir = i.unwrap();
            let pkg = dir.path().join("cmdline");

            let file = fs::read_to_string(pkg).unwrap();

            if package_name == file {
                pids.push(
                    dir.file_name()
                        .to_string_lossy()
                        .parse::<isize>()
                        .unwrap_or(0),
                );
            }
        }

        // hook Native
        let mname = CString::new("nSyncAndDrawFrame").unwrap();
        let msig = CString::new("(J[JI)I").unwrap();

        let env_ptr = self.env.get_native_interface() as *mut *const JNINativeInterface_;

        let methods = [JNINativeMethod {
            name: mname.into_raw(),
            signature: msig.into_raw(),
            fnPtr: nsync as *mut c_void,
        }];
        self.api
            .hook_jni_native_methods(env_ptr, "android/graphics/HardwareRenderer", methods);
        if methods[0].fnPtr.is_null() {
            log::error!("[zygisk-rs] hookJniNativeMethods failed!");
        } else {
            log::info!(
                "[zygisk-rs] Hooked! orig={:p}, pids={pids:?}",
                methods[0].fnPtr
            );
            unsafe {
                *ORIG.lock().unwrap() = Some(std::mem::transmute(methods[0].fnPtr));
            }
        }
    }

    fn post_app_specialize(&mut self, _args: &zygisk_rs::AppSpecializeArgs) {}

    fn pre_server_specialize(&mut self, _args: &mut zygisk_rs::ServerSpecializeArgs) {}

    fn post_server_specialize(&mut self, _args: &zygisk_rs::ServerSpecializeArgs) {}
}

register_zygisk_module!(Zygisk);
