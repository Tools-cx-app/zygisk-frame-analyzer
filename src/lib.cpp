#include <android/log.h>
#include <jni.h>
#include <pthread.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/un.h>
#include <time.h>
#include <unistd.h>

#include <atomic>
#include <cstddef>
#include <cstdio>
#include <cstdlib>

#include "zygisk.hpp"

#define LOGD(...) \
    __android_log_print(ANDROID_LOG_DEBUG, "zygisk-frame-analyzer", __VA_ARGS__)

using zygisk::Api;
using zygisk::AppSpecializeArgs;
using zygisk::Option;

const std::size_t VSYNC = 3;
const char *dir = "/data/adb/modules/fas_rs_next";

static std::atomic<long> g_lastFrameTime{0};

static int (*orig_func)(JNIEnv *env, jobject clazz, jlong proxyPtr,
                        jlongArray frameInfo, jint frameInfoSize);
static int my_func(JNIEnv *env, jobject clazz, jlong proxyPtr,
                   jlongArray frameInfo, jint frameInfoSize) {
    jsize len = env->GetArrayLength(frameInfo);
    if (len < 4)
        return orig_func(env, clazz, proxyPtr, frameInfo, frameInfoSize);

    jlong buffer[4];
    env->GetLongArrayRegion(frameInfo, 0, 4, buffer);

    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    jlong now = ts.tv_sec * 1000000000LL + ts.tv_nsec;
    jlong delta = now - buffer[VSYNC];
    g_lastFrameTime.store(delta, std::memory_order_relaxed);
    pid_t pid = getpid();

    LOGD("frametime: %ld, pid: %d", delta, pid);
    return orig_func(env, clazz, proxyPtr, frameInfo, frameInfoSize);
}

bool ensure_socket_access(const char *sock_path) {
    if (mkdir(dir, 0700) != 0 && errno != EEXIST) {
        LOGD("mkdir failed");
        return false;
    }

    if (chmod(dir, 0700) != 0) {
        LOGD("chmod dir failed");
        return false;
    }

    if (chmod(sock_path, 0666) != 0) {
        LOGD("chmod sock failed");
        return false;
    }

    char cmd[256];
    chmod(sock_path, 0666);
    snprintf(cmd, sizeof(cmd), "chcon u:object_r:adb_data_file:s0 %s",
             sock_path);
    system(cmd);

    snprintf(cmd, sizeof(cmd), "chcon -R u:object_r:adb_data_file:s0 %s", dir);
    system(cmd);

    return true;
}

static void *server_thread(void *) {
    char sock_path[256];
    snprintf(sock_path, sizeof(sock_path), "%s/zygisk.sock", dir);

    ensure_socket_access(sock_path);
    int fd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (fd < 0) {
        LOGD("socket failed: %s", strerror(errno));
        return nullptr;
    }

    unlink(sock_path);

    struct sockaddr_un addr{};
    addr.sun_family = AF_UNIX;
    strncpy(addr.sun_path, sock_path, sizeof(addr.sun_path) - 1);

    if (bind(fd, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        LOGD("bind %s failed: %s", sock_path, strerror(errno));
        close(fd);
        return nullptr;
    }
    chmod(sock_path, 0666);

    if (listen(fd, 1) < 0) {
        LOGD("listen failed: %s", strerror(errno));
        close(fd);
        return nullptr;
    }

    LOGD("Unix socket server ready @ %s", sock_path);

    while (true) {
        int c = accept(fd, nullptr, nullptr);
        if (c < 0) continue;

        long t = g_lastFrameTime.load(std::memory_order_relaxed);
        char reply[64];
        int len = snprintf(reply, sizeof(reply), "%d:%ld\n", getpid(), t);
        send(c, reply, len, MSG_NOSIGNAL);
        close(c);
    }
    return nullptr;
}

class Zygisk : public zygisk::ModuleBase {
   public:
    void onLoad(Api *api, JNIEnv *env) override {
        this->api = api;
        this->env = env;
    }

    void postAppSpecialize(const AppSpecializeArgs *args) override {
        pthread_t tid;
        pthread_create(&tid, nullptr, server_thread, nullptr);
        pthread_detach(tid);
        LOGD("socket server thread started");
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

REGISTER_ZYGISK_MODULE(Zygisk)
