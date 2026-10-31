# Links the JNI glue and the C library into one shared library per ABI.
#
# The C library is linked as its static archive rather than loaded as the
# cdylib cargo also builds. One .so means one System.loadLibrary call, a
# DT_SONAME the Android loader expects (the Rust cdylib for Android has none),
# and --gc-sections can drop every C ABI function the glue never calls, such
# as the CPython and Node entry points.
#
# INILLUCENT_RUST_TARGET_ROOT is cargo's target directory, passed in by
# inillucent/build.gradle.kts, whose cargo task for this ABI has already run.
# The archive is under <root>/<rust triple>/release.

cmake_minimum_required(VERSION 3.22.1)
project(inillucent_jni C)

if(NOT INILLUCENT_RUST_TARGET_ROOT)
  message(FATAL_ERROR "INILLUCENT_RUST_TARGET_ROOT is not set; build through Gradle, which runs cargo first")
endif()

if(ANDROID_ABI STREQUAL "arm64-v8a")
  set(INILLUCENT_RUST_TRIPLE aarch64-linux-android)
elseif(ANDROID_ABI STREQUAL "armeabi-v7a")
  set(INILLUCENT_RUST_TRIPLE armv7-linux-androideabi)
elseif(ANDROID_ABI STREQUAL "x86_64")
  set(INILLUCENT_RUST_TRIPLE x86_64-linux-android)
else()
  message(FATAL_ERROR "no Rust target for the Android ABI ${ANDROID_ABI}")
endif()
set(INILLUCENT_RUST_LIB_DIR "${INILLUCENT_RUST_TARGET_ROOT}/${INILLUCENT_RUST_TRIPLE}/release")

add_library(inillucent_driver_capi STATIC IMPORTED)
set_target_properties(inillucent_driver_capi PROPERTIES
  IMPORTED_LOCATION "${INILLUCENT_RUST_LIB_DIR}/libinillucent_driver_capi.a")

add_library(inillucent_jni SHARED inillucent_jni.c)
target_include_directories(inillucent_jni PRIVATE "${INILLUCENT_INCLUDE_DIR}")
target_compile_options(inillucent_jni PRIVATE -Wall -Wextra -Werror -fvisibility=hidden)
target_link_libraries(inillucent_jni PRIVATE inillucent_driver_capi log m dl)
# 16 KB pages: Android 15 devices can use them, and Google Play refuses an app
# targeting API 35 whose native libraries are not aligned to them. NDK r28
# aligns by default; r27, which this repository builds with, does not.
# packages/android/run-tests.sh checks every LOAD segment of the packaged
# library.
target_link_options(inillucent_jni PRIVATE
  "-Wl,-z,max-page-size=16384"
  "-Wl,--gc-sections"
  "-Wl,--exclude-libs,ALL")
