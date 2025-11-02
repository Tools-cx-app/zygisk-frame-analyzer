#include <android/log.h>
#include <jni.h>
#include <string.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>

#include "zygisk.hpp"

#define LOGD(...) \
    __android_log_print(ANDROID_LOG_DEBUG, "zygisk-frame-analyzer", __VA_ARGS__)

using zygisk::Api;
using zygisk::AppSpecializeArgs;
using zygisk::Option;

const size_t VSYNC = 3;

static int (*orig_func)(JNIEnv *env, jobject clazz, jlong proxyPtr,
                        jlongArray frameInfo, jint frameInfoSize);
static int my_func(JNIEnv *env, jobject clazz, jlong proxyPtr,
                   jlongArray frameInfo, jint frameInfoSize) {
    jlong buffer[frameInfoSize];
    env->GetLongArrayRegion(frameInfo, 0, frameInfoSize, buffer);

    struct timespec currentTime;
    clock_gettime(CLOCK_MONOTONIC, &currentTime);
    jlong currentNanos =
        currentTime.tv_sec * 1000000000LL + currentTime.tv_nsec;
    jlong vsyncNanos = buffer[VSYNC];

    LOGD("frametime: %ld", currentNanos - vsyncNanos);

    return orig_func(env, clazz, proxyPtr, frameInfo, frameInfoSize);
}

class Demo : public zygisk::ModuleBase {
   public:
    void onLoad(Api *api, JNIEnv *env) override {
        this->api = api;
        this->env = env;
    }

    void preAppSpecialize(AppSpecializeArgs *args) override {
        JNINativeMethod methods[] = {
            {"nSyncAndDrawFrame", "(J[JI)I", (void *)my_func},
        };

        api->hookJniNativeMethods(env, "android/graphics/HardwareRenderer",
                                  methods, 1);
        *(void **)&orig_func = methods[0].fnPtr;

        if (methods[0].fnPtr == nullptr) {
            LOGD("Failed to hook");
        } else {
            LOGD("Hooked");
        }
    }

   private:
    Api *api;
    JNIEnv *env;
};

REGISTER_ZYGISK_MODULE(Demo)
