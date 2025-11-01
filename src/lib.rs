use jni::JNIEnv;
use jni_sys::{JNINativeInterface_, JNINativeMethod, jbyteArray, jint, jlong};
use std::ffi::{CString, c_void};
use std::sync::{LazyLock, Mutex};
use zygisk_rs::{Api, Module};

static ORIG: LazyLock<Mutex<Option<extern "C" fn(jlong, jbyteArray, jint) -> jint>>> =
    LazyLock::new(|| Mutex::new(None));

extern "C" fn my_nSyncAndDrawFrame(_arg0: jlong, _arg1: jbyteArray, _arg2: jint) -> jint {
    log::info!("[zygisk-rs] nSyncAndDrawFrame hit!");
    let orig = ORIG.lock().unwrap();
    if let Some(f) = *orig {
        drop(orig);
        { f(_arg0, _arg1, _arg2) }
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

    fn pre_app_specialize(&mut self, _args: &mut zygisk_rs::AppSpecializeArgs) {
        let mname = CString::new("nSyncAndDrawFrame").unwrap();
        let msig = CString::new("(J[JI)I").unwrap();

        let env_ptr = self.env.get_native_interface() as *mut *const JNINativeInterface_;

        let methods = [JNINativeMethod {
            name: mname.into_raw(),
            signature: msig.into_raw(),
            fnPtr: my_nSyncAndDrawFrame as *mut c_void,
        }];
        self.api
            .hook_jni_native_methods(env_ptr, "android/graphics/HardwareRenderer", methods);
        // 3.5 保存原始指针
        if methods[0].fnPtr.is_null() {
            log::error!("[zygisk-rs] hookJniNativeMethods failed!");
        } else {
            log::info!("[zygisk-rs] Hooked! orig={:p}", methods[0].fnPtr);
            unsafe {
                *ORIG.lock().unwrap() = Some(std::mem::transmute(methods[0].fnPtr));
            }
        }
    }

    fn post_app_specialize(&mut self, _args: &zygisk_rs::AppSpecializeArgs) {}

    fn pre_server_specialize(&mut self, _args: &mut zygisk_rs::ServerSpecializeArgs) {}

    fn post_server_specialize(&mut self, _args: &zygisk_rs::ServerSpecializeArgs) {}
}
