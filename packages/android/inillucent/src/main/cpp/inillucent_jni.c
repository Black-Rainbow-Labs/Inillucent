/*
 * inillucent_jni.c - the JNI glue between the Kotlin classes in
 * com.blackrainbowlabs.inillucent and the C library in inillucent_driver.h.
 *
 * WHAT THIS FILE DECIDES: NOTHING
 *
 * Every behavior is the C library's. This file moves handles, bytes and
 * numbers across JNI and turns a failure into a Java exception. The Kotlin
 * classes decide what the exception is and how a value looks in Kotlin, so a
 * change to the API is a change to Kotlin and not to C.
 *
 * WHY EVERY STRING CROSSES AS byte[]
 *
 * JNI's NewStringUTF and GetStringUTFChars read and write "modified UTF-8".
 * It encodes a character outside the Basic Multilingual Plane as two three
 * byte surrogates instead of one four byte sequence, and it encodes a NUL as
 * two bytes. The engine stores real UTF-8, and the conformance suite holds both
 * an emoji and a NUL byte inside text. A binding that used NewStringUTF would
 * hand an application a different string from the one it stored. So text goes
 * both ways as a byte[] of UTF-8 and Kotlin decodes and encodes it.
 *
 * HANDLES
 *
 * A handle is a pointer carried in a jlong. Kotlin owns each one and frees it
 * exactly once; the C library makes freeing NULL a no-op.
 *
 * THREADS
 *
 * The engine is single threaded per file. Kotlin's Database holds the lock
 * that serialises every call except cancel, so nothing here locks.
 */

#include <jni.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "inillucent_driver.h"

/* The classes and the one method this file calls back into, looked up once in
 * JNI_OnLoad. A class looked up later from a thread the application started
 * would be resolved by the system class loader, which cannot see the
 * application's classes. */
static jclass byte_array_class;
static jclass object_class;
static jclass native_class;
static jmethodID failure_method;

#define NATIVE(name) Java_com_blackrainbowlabs_inillucent_Native_##name

/**
 * Caches the classes this file needs while the loader that can see them is
 * the current one.
 * @param vm - the virtual machine loading the library
 * @param reserved - unused
 */
JNIEXPORT jint JNI_OnLoad(JavaVM *vm, void *reserved) {
    (void)reserved;
    JNIEnv *env;
    if ((*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6) != JNI_OK) {
        return JNI_ERR;
    }
    jclass found = (*env)->FindClass(env, "[B");
    if (found == NULL) return JNI_ERR;
    byte_array_class = (*env)->NewGlobalRef(env, found);
    found = (*env)->FindClass(env, "java/lang/Object");
    if (found == NULL) return JNI_ERR;
    object_class = (*env)->NewGlobalRef(env, found);
    found = (*env)->FindClass(env, "com/blackrainbowlabs/inillucent/Native");
    if (found == NULL) return JNI_ERR;
    native_class = (*env)->NewGlobalRef(env, found);
    failure_method = (*env)->GetStaticMethodID(
        env, native_class, "failure", "(I[B[B[BI[Ljava/lang/Object;)Ljava/lang/Throwable;");
    if (failure_method == NULL) return JNI_ERR;
    return JNI_VERSION_1_6;
}

/**
 * Copies bytes into a new Java byte[], or returns NULL for a NULL pointer.
 * @param env - the JNI environment
 * @param data - the bytes, or NULL
 * @param len - how many
 */
static jbyteArray to_bytes(JNIEnv *env, const void *data, size_t len) {
    if (data == NULL) return NULL;
    jbyteArray out = (*env)->NewByteArray(env, (jsize)len);
    if (out != NULL && len > 0) {
        (*env)->SetByteArrayRegion(env, out, 0, (jsize)len, (const jbyte *)data);
    }
    return out;
}

/**
 * Copies a C string into a new Java byte[], or NULL for NULL.
 * @param env - the JNI environment
 * @param text - a NUL terminated string, or NULL
 */
static jbyteArray c_bytes(JNIEnv *env, const char *text) {
    return text == NULL ? NULL : to_bytes(env, text, strlen(text));
}

/**
 * Copies a Java byte[] into a NUL terminated buffer the caller frees, or
 * returns NULL for a NULL array. Kotlin refuses a NUL inside SQL or a path
 * before it gets here, because the C library reads those as C strings.
 * @param env - the JNI environment
 * @param bytes - the UTF-8 bytes
 */
static char *to_cstring(JNIEnv *env, jbyteArray bytes) {
    if (bytes == NULL) return NULL;
    jsize len = (*env)->GetArrayLength(env, bytes);
    char *out = malloc((size_t)len + 1);
    if (out == NULL) return NULL;
    (*env)->GetByteArrayRegion(env, bytes, 0, len, (jbyte *)out);
    out[len] = '\0';
    return out;
}

/**
 * Reads a result into the Object[] Kotlin's Rows.read expects, then frees it.
 *
 * One call for the whole result rather than one per cell: a JNI call costs
 * far more than the copy, and the C library has already materialised the
 * rows. The layout is positional, and Rows.read in Kotlin is its only reader:
 *   0 byte[][] column names   1 byte[][] declared types
 *   2 int[] kinds             3 long[] integers   4 double[] reals
 *   5 byte[][] text and blob bytes, NULL for the other kinds
 *   6 long[] { count, columns, total, more, affected, elapsed_us }
 *   7 byte[] tag
 * Cells are in row major order.
 * @param env - the JNI environment
 * @param rows - the result, which this frees
 */
static jobjectArray read_rows(JNIEnv *env, inillucent_rows *rows) {
    size_t columns = inillucent_rows_column_count(rows);
    size_t count = inillucent_rows_count(rows);
    size_t cells = columns * count;
    jobjectArray out = (*env)->NewObjectArray(env, 8, object_class, NULL);
    jobjectArray names = (*env)->NewObjectArray(env, (jsize)columns, byte_array_class, NULL);
    jobjectArray types = (*env)->NewObjectArray(env, (jsize)columns, byte_array_class, NULL);
    jintArray kinds = (*env)->NewIntArray(env, (jsize)cells);
    jlongArray ints = (*env)->NewLongArray(env, (jsize)cells);
    jdoubleArray reals = (*env)->NewDoubleArray(env, (jsize)cells);
    jobjectArray bytes = (*env)->NewObjectArray(env, (jsize)cells, byte_array_class, NULL);
    jlongArray meta = (*env)->NewLongArray(env, 6);
    if (out == NULL || names == NULL || types == NULL || kinds == NULL || ints == NULL ||
        reals == NULL || bytes == NULL || meta == NULL) {
        inillucent_rows_free(rows);
        return NULL; /* an OutOfMemoryError is pending */
    }
    for (size_t c = 0; c < columns; c++) {
        jbyteArray name = c_bytes(env, inillucent_rows_column_name(rows, c));
        jbyteArray type = c_bytes(env, inillucent_rows_column_type(rows, c));
        (*env)->SetObjectArrayElement(env, names, (jsize)c, name);
        (*env)->SetObjectArrayElement(env, types, (jsize)c, type);
        (*env)->DeleteLocalRef(env, name);
        (*env)->DeleteLocalRef(env, type);
    }
    jint *kind = (*env)->GetIntArrayElements(env, kinds, NULL);
    jlong *integer = (*env)->GetLongArrayElements(env, ints, NULL);
    jdouble *real = (*env)->GetDoubleArrayElements(env, reals, NULL);
    for (size_t r = 0; r < count; r++) {
        for (size_t c = 0; c < columns; c++) {
            size_t at = r * columns + c;
            int32_t type = inillucent_value_type(rows, r, c);
            kind[at] = type;
            integer[at] = 0;
            real[at] = 0.0;
            if (type == INILLUCENT_INTEGER) {
                integer[at] = inillucent_value_int(rows, r, c);
            } else if (type == INILLUCENT_REAL) {
                real[at] = inillucent_value_real(rows, r, c);
            } else if (type == INILLUCENT_TEXT || type == INILLUCENT_BLOB) {
                size_t len = 0;
                const uint8_t *data = inillucent_value_bytes(rows, r, c, &len);
                /* An empty text or blob comes back as a NULL pointer with
                 * length 0 from some paths; it is still a value, not a NULL. */
                static const uint8_t empty = 0;
                jbyteArray cell = to_bytes(env, data == NULL ? &empty : data, len);
                (*env)->SetObjectArrayElement(env, bytes, (jsize)at, cell);
                (*env)->DeleteLocalRef(env, cell);
            }
        }
    }
    (*env)->ReleaseIntArrayElements(env, kinds, kind, 0);
    (*env)->ReleaseLongArrayElements(env, ints, integer, 0);
    (*env)->ReleaseDoubleArrayElements(env, reals, real, 0);
    jlong numbers[6] = {
        (jlong)count,
        (jlong)columns,
        (jlong)inillucent_rows_total(rows),
        (jlong)inillucent_rows_more(rows),
        (jlong)inillucent_rows_affected(rows),
        (jlong)inillucent_rows_elapsed_us(rows),
    };
    (*env)->SetLongArrayRegion(env, meta, 0, 6, numbers);
    jbyteArray tag = c_bytes(env, inillucent_rows_tag(rows));
    inillucent_rows_free(rows);
    jobject parts[8] = {names, types, kinds, ints, reals, bytes, meta, tag};
    for (jsize i = 0; i < 8; i++) {
        (*env)->SetObjectArrayElement(env, out, i, parts[i]);
        (*env)->DeleteLocalRef(env, parts[i]);
    }
    return out;
}

/**
 * Throws the Kotlin exception for a failed call, built by Native.failure, and
 * frees the error. Any rows the statement produced before it failed travel
 * with the exception, the way SQLite hands them out.
 * @param env - the JNI environment
 * @param status - the status the call returned
 * @param error - the error the call wrote, or NULL when the call has none
 */
static void throw_failure(JNIEnv *env, int32_t status, inillucent_error *error) {
    jbyteArray message = NULL, feature = NULL, detail = NULL;
    jobjectArray partial = NULL;
    jint offset = -1;
    if (error != NULL) {
        status = inillucent_error_status(error);
        message = c_bytes(env, inillucent_error_message(error));
        feature = c_bytes(env, inillucent_error_feature(error));
        detail = c_bytes(env, inillucent_error_detail(error));
        offset = inillucent_error_offset(error);
        inillucent_rows *rows = inillucent_error_take_rows(error);
        if (rows != NULL) partial = read_rows(env, rows);
        inillucent_error_free(error);
    }
    if ((*env)->ExceptionCheck(env)) return;
    jobject thrown = (*env)->CallStaticObjectMethod(
        env, native_class, failure_method, (jint)status, message, feature, detail, offset, partial);
    if (thrown != NULL && !(*env)->ExceptionCheck(env)) {
        (*env)->Throw(env, (jthrowable)thrown);
    }
}

/**
 * Throws for a status with no error object, which is how the bind calls and
 * cancel report.
 * @param env - the JNI environment
 * @param status - the status
 * @param what - a C string naming what failed
 */
static void throw_status(JNIEnv *env, int32_t status, const char *what) {
    jbyteArray message = c_bytes(env, what);
    jobject thrown = (*env)->CallStaticObjectMethod(
        env, native_class, failure_method, (jint)status, message, NULL, NULL, (jint)-1, NULL);
    if (thrown != NULL && !(*env)->ExceptionCheck(env)) {
        (*env)->Throw(env, (jthrowable)thrown);
    }
}

/** A negative Kotlin limit means every row. @param limit - the limit */
static uint64_t row_limit(jlong limit) {
    return limit < 0 ? UINT64_MAX : (uint64_t)limit;
}

/* ---------------------------------------------------------------- */
/* The library.                                                      */
/* ---------------------------------------------------------------- */

/** The ABI version, major*1000000 + minor*1000 + patch. */
JNIEXPORT jint JNICALL NATIVE(abiVersion)(JNIEnv *env, jclass cls) {
    (void)env; (void)cls;
    return (jint)inillucent_abi_version();
}

/** What the driver calls itself, as UTF-8. */
JNIEXPORT jbyteArray JNICALL NATIVE(version)(JNIEnv *env, jclass cls) {
    (void)cls;
    return c_bytes(env, inillucent_version());
}

/** How many capabilities this build declares. */
JNIEXPORT jint JNICALL NATIVE(capabilityCount)(JNIEnv *env, jclass cls) {
    (void)env; (void)cls;
    return (jint)inillucent_capability_count();
}

/**
 * One capability as Object[] { byte[] name, byte[] note, int[] { state } }.
 * @param nth - its position
 */
JNIEXPORT jobjectArray JNICALL NATIVE(capability)(JNIEnv *env, jclass cls, jint nth) {
    (void)cls;
    const char *name = NULL, *note = NULL;
    int32_t state = INILLUCENT_SUPPORT_UNKNOWN;
    int32_t status = inillucent_capability((size_t)nth, &name, &state, &note);
    if (status != INILLUCENT_OK) {
        throw_status(env, status, "no capability at that position");
        return NULL;
    }
    jobjectArray out = (*env)->NewObjectArray(env, 3, object_class, NULL);
    if (out == NULL) return NULL;
    jintArray held = (*env)->NewIntArray(env, 1);
    if (held == NULL) return NULL;
    jint value = state;
    (*env)->SetIntArrayRegion(env, held, 0, 1, &value);
    (*env)->SetObjectArrayElement(env, out, 0, c_bytes(env, name));
    (*env)->SetObjectArrayElement(env, out, 1, c_bytes(env, note));
    (*env)->SetObjectArrayElement(env, out, 2, held);
    return out;
}

/** Looks a capability up by name. @param name - its UTF-8 name */
JNIEXPORT jint JNICALL NATIVE(supports)(JNIEnv *env, jclass cls, jbyteArray name) {
    (void)cls;
    char *text = to_cstring(env, name);
    if (text == NULL) return INILLUCENT_SUPPORT_UNKNOWN;
    int32_t state = inillucent_supports(text);
    free(text);
    return state;
}

/* ---------------------------------------------------------------- */
/* A database.                                                       */
/* ---------------------------------------------------------------- */

/**
 * Opens a database and returns its handle.
 * @param path - the file, as UTF-8
 * @param flags - INILLUCENT_OPEN_* flags
 * @param key - the key as UTF-8, or NULL for an unencrypted file
 * @param timeout_ms - the busy timeout, or a negative number for the default
 */
JNIEXPORT jlong JNICALL NATIVE(open)(JNIEnv *env, jclass cls, jbyteArray path, jint flags,
                                     jbyteArray key, jint timeout_ms) {
    (void)cls;
    char *file = to_cstring(env, path);
    char *secret = to_cstring(env, key);
    inillucent_db *db = NULL;
    inillucent_error *error = NULL;
    int32_t status;
    if (secret != NULL) {
        status = inillucent_open_with_key(file, (uint32_t)flags, secret, &db, &error);
    } else if (timeout_ms >= 0) {
        status = inillucent_open_with_timeout(file, (uint32_t)flags, (uint32_t)timeout_ms, &db,
                                              &error);
    } else {
        status = inillucent_open(file, (uint32_t)flags, &db, &error);
    }
    if (secret != NULL) {
        /* The key is a secret; do not leave it in a freed heap block. */
        memset(secret, 0, strlen(secret));
        free(secret);
    }
    free(file);
    if (status != INILLUCENT_OK) {
        throw_failure(env, status, error);
        return 0;
    }
    return (jlong)(intptr_t)db;
}

/** Closes a database. @param db - its handle */
JNIEXPORT void JNICALL NATIVE(close)(JNIEnv *env, jclass cls, jlong db) {
    (void)cls;
    inillucent_error *error = NULL;
    int32_t status = inillucent_close((inillucent_db *)(intptr_t)db, &error);
    if (status != INILLUCENT_OK) throw_failure(env, status, error);
}

/** Makes everything written so far durable. @param db - its handle */
JNIEXPORT void JNICALL NATIVE(checkpoint)(JNIEnv *env, jclass cls, jlong db) {
    (void)cls;
    inillucent_error *error = NULL;
    int32_t status = inillucent_checkpoint((inillucent_db *)(intptr_t)db, &error);
    if (status != INILLUCENT_OK) throw_failure(env, status, error);
}

/** Walks every tree and throws on the first fault. @param db - its handle */
JNIEXPORT void JNICALL NATIVE(integrityCheck)(JNIEnv *env, jclass cls, jlong db) {
    (void)cls;
    inillucent_error *error = NULL;
    int32_t status = inillucent_integrity_check((inillucent_db *)(intptr_t)db, &error);
    if (status != INILLUCENT_OK) throw_failure(env, status, error);
}

/**
 * Copies the database to a file and checks the copy.
 * @param db - its handle
 * @param path - the destination, as UTF-8
 */
JNIEXPORT void JNICALL NATIVE(backupTo)(JNIEnv *env, jclass cls, jlong db, jbyteArray path) {
    (void)cls;
    char *file = to_cstring(env, path);
    inillucent_error *error = NULL;
    int32_t status = inillucent_backup_to((inillucent_db *)(intptr_t)db, file, &error);
    free(file);
    if (status != INILLUCENT_OK) throw_failure(env, status, error);
}

/** The file the database is in, as UTF-8. @param db - its handle */
JNIEXPORT jbyteArray JNICALL NATIVE(path)(JNIEnv *env, jclass cls, jlong db) {
    (void)cls;
    return c_bytes(env, inillucent_path((const inillucent_db *)(intptr_t)db));
}

/* ---------------------------------------------------------------- */
/* A connection.                                                     */
/* ---------------------------------------------------------------- */

/** Opens a connection. @param db - the database's handle */
JNIEXPORT jlong JNICALL NATIVE(connect)(JNIEnv *env, jclass cls, jlong db) {
    (void)cls;
    inillucent_conn *conn = NULL;
    inillucent_error *error = NULL;
    int32_t status = inillucent_connect((inillucent_db *)(intptr_t)db, &conn, &error);
    if (status != INILLUCENT_OK) {
        throw_failure(env, status, error);
        return 0;
    }
    return (jlong)(intptr_t)conn;
}

/** Frees a connection handle. @param conn - its handle */
JNIEXPORT void JNICALL NATIVE(connFree)(JNIEnv *env, jclass cls, jlong conn) {
    (void)env; (void)cls;
    inillucent_conn_free((inillucent_conn *)(intptr_t)conn);
}

/**
 * Runs one statement with nothing bound and returns its result.
 * @param conn - the connection
 * @param sql - the statement, as UTF-8
 * @param limit - rows to hand back, negative for all
 */
JNIEXPORT jobjectArray JNICALL NATIVE(execute)(JNIEnv *env, jclass cls, jlong conn,
                                               jbyteArray sql, jlong limit) {
    (void)cls;
    char *text = to_cstring(env, sql);
    inillucent_rows *rows = NULL;
    inillucent_error *error = NULL;
    int32_t status = inillucent_execute((inillucent_conn *)(intptr_t)conn, text, row_limit(limit),
                                        &rows, &error);
    free(text);
    if (status != INILLUCENT_OK) {
        throw_failure(env, status, error);
        return NULL;
    }
    return read_rows(env, rows);
}

/**
 * Runs statements separated by semicolons for their effect.
 * @param conn - the connection
 * @param sql - the statements, as UTF-8
 */
JNIEXPORT void JNICALL NATIVE(executeBatch)(JNIEnv *env, jclass cls, jlong conn, jbyteArray sql) {
    (void)cls;
    char *text = to_cstring(env, sql);
    inillucent_error *error = NULL;
    int32_t status = inillucent_execute_batch((inillucent_conn *)(intptr_t)conn, text, &error);
    free(text);
    if (status != INILLUCENT_OK) throw_failure(env, status, error);
}

/** The rowid of the last insert. @param conn - the connection */
JNIEXPORT jlong JNICALL NATIVE(lastInsertRowid)(JNIEnv *env, jclass cls, jlong conn) {
    (void)env; (void)cls;
    return inillucent_last_insert_rowid((inillucent_conn *)(intptr_t)conn);
}

/** Rows changed since the connection opened. @param conn - the connection */
JNIEXPORT jlong JNICALL NATIVE(totalChanges)(JNIEnv *env, jclass cls, jlong conn) {
    (void)env; (void)cls;
    return inillucent_total_changes((inillucent_conn *)(intptr_t)conn);
}

/** Whether a transaction is open. @param conn - the connection */
JNIEXPORT jboolean JNICALL NATIVE(inTransaction)(JNIEnv *env, jclass cls, jlong conn) {
    (void)env; (void)cls;
    return inillucent_in_transaction((inillucent_conn *)(intptr_t)conn) ? JNI_TRUE : JNI_FALSE;
}

/** The schema's generation. @param conn - the connection */
JNIEXPORT jlong JNICALL NATIVE(schemaCookie)(JNIEnv *env, jclass cls, jlong conn) {
    (void)env; (void)cls;
    return (jlong)inillucent_schema_cookie((inillucent_conn *)(intptr_t)conn);
}

/** Asks the running statement to stop; safe from any thread. @param conn - the connection */
JNIEXPORT void JNICALL NATIVE(cancel)(JNIEnv *env, jclass cls, jlong conn) {
    (void)cls;
    inillucent_error *error = NULL;
    int32_t status = inillucent_cancel((inillucent_conn *)(intptr_t)conn, &error);
    if (status != INILLUCENT_OK) throw_failure(env, status, error);
}

/* ---------------------------------------------------------------- */
/* A statement.                                                      */
/* ---------------------------------------------------------------- */

/**
 * Prepares a statement.
 * @param conn - the connection
 * @param sql - the statement, as UTF-8
 */
JNIEXPORT jlong JNICALL NATIVE(prepare)(JNIEnv *env, jclass cls, jlong conn, jbyteArray sql) {
    (void)cls;
    char *text = to_cstring(env, sql);
    inillucent_stmt *stmt = NULL;
    inillucent_error *error = NULL;
    int32_t status = inillucent_prepare((inillucent_conn *)(intptr_t)conn, text, &stmt, &error);
    free(text);
    if (status != INILLUCENT_OK) {
        throw_failure(env, status, error);
        return 0;
    }
    return (jlong)(intptr_t)stmt;
}

/** Frees a statement. @param stmt - its handle */
JNIEXPORT void JNICALL NATIVE(stmtFree)(JNIEnv *env, jclass cls, jlong stmt) {
    (void)env; (void)cls;
    inillucent_stmt_free((inillucent_stmt *)(intptr_t)stmt);
}

/** Unbinds every parameter. @param stmt - its handle */
JNIEXPORT void JNICALL NATIVE(clearBindings)(JNIEnv *env, jclass cls, jlong stmt) {
    (void)env; (void)cls;
    inillucent_clear_bindings((inillucent_stmt *)(intptr_t)stmt);
}

/** Binds NULL. @param stmt - the statement @param index - one based */
JNIEXPORT void JNICALL NATIVE(bindNull)(JNIEnv *env, jclass cls, jlong stmt, jint index) {
    (void)cls;
    int32_t status = inillucent_bind_null((inillucent_stmt *)(intptr_t)stmt, (uint32_t)index);
    if (status != INILLUCENT_OK) throw_status(env, status, "the parameter could not be bound");
}

/** Binds an integer. @param stmt - the statement @param index - one based @param value - it */
JNIEXPORT void JNICALL NATIVE(bindInt)(JNIEnv *env, jclass cls, jlong stmt, jint index,
                                       jlong value) {
    (void)cls;
    int32_t status =
        inillucent_bind_int((inillucent_stmt *)(intptr_t)stmt, (uint32_t)index, value);
    if (status != INILLUCENT_OK) throw_status(env, status, "the parameter could not be bound");
}

/** Binds a real. @param stmt - the statement @param index - one based @param value - it */
JNIEXPORT void JNICALL NATIVE(bindReal)(JNIEnv *env, jclass cls, jlong stmt, jint index,
                                        jdouble value) {
    (void)cls;
    int32_t status =
        inillucent_bind_real((inillucent_stmt *)(intptr_t)stmt, (uint32_t)index, value);
    if (status != INILLUCENT_OK) throw_status(env, status, "the parameter could not be bound");
}

/**
 * Binds text or a blob from a byte[], which the C library copies.
 * @param env - the JNI environment
 * @param stmt - the statement
 * @param index - one based
 * @param value - the bytes
 * @param blob - whether it is a blob rather than UTF-8 text
 */
static void bind_bytes(JNIEnv *env, jlong stmt, jint index, jbyteArray value, int blob) {
    jsize len = (*env)->GetArrayLength(env, value);
    jbyte *data = (*env)->GetByteArrayElements(env, value, NULL);
    if (data == NULL) return;
    int32_t status = blob
        ? inillucent_bind_blob((inillucent_stmt *)(intptr_t)stmt, (uint32_t)index,
                               (const uint8_t *)data, (size_t)len)
        : inillucent_bind_text((inillucent_stmt *)(intptr_t)stmt, (uint32_t)index,
                               (const char *)data, (size_t)len);
    (*env)->ReleaseByteArrayElements(env, value, data, JNI_ABORT);
    if (status != INILLUCENT_OK) throw_status(env, status, "the parameter could not be bound");
}

/** Binds UTF-8 text. @param stmt - the statement @param index - one based @param value - bytes */
JNIEXPORT void JNICALL NATIVE(bindText)(JNIEnv *env, jclass cls, jlong stmt, jint index,
                                        jbyteArray value) {
    (void)cls;
    bind_bytes(env, stmt, index, value, 0);
}

/** Binds a blob. @param stmt - the statement @param index - one based @param value - bytes */
JNIEXPORT void JNICALL NATIVE(bindBlob)(JNIEnv *env, jclass cls, jlong stmt, jint index,
                                        jbyteArray value) {
    (void)cls;
    bind_bytes(env, stmt, index, value, 1);
}

/**
 * Runs a statement with what is bound.
 * @param stmt - the statement
 * @param limit - rows to hand back, negative for all
 */
JNIEXPORT jobjectArray JNICALL NATIVE(stmtExecute)(JNIEnv *env, jclass cls, jlong stmt,
                                                   jlong limit) {
    (void)cls;
    inillucent_rows *rows = NULL;
    inillucent_error *error = NULL;
    int32_t status =
        inillucent_stmt_execute((inillucent_stmt *)(intptr_t)stmt, row_limit(limit), &rows, &error);
    if (status != INILLUCENT_OK) {
        throw_failure(env, status, error);
        return NULL;
    }
    return read_rows(env, rows);
}

/* ---------------------------------------------------------------- */
/* A transaction.                                                    */
/* ---------------------------------------------------------------- */

/** Begins a transaction. @param conn - the connection */
JNIEXPORT jlong JNICALL NATIVE(txnBegin)(JNIEnv *env, jclass cls, jlong conn) {
    (void)cls;
    inillucent_txn *txn = NULL;
    inillucent_error *error = NULL;
    int32_t status = inillucent_txn_begin((inillucent_conn *)(intptr_t)conn, &txn, &error);
    if (status != INILLUCENT_OK) {
        throw_failure(env, status, error);
        return 0;
    }
    return (jlong)(intptr_t)txn;
}

/**
 * Runs one statement inside the transaction and returns rows changed.
 * @param txn - the transaction
 * @param sql - the statement, as UTF-8
 */
JNIEXPORT jlong JNICALL NATIVE(txnExecute)(JNIEnv *env, jclass cls, jlong txn, jbyteArray sql) {
    (void)cls;
    char *text = to_cstring(env, sql);
    uint64_t affected = 0;
    inillucent_error *error = NULL;
    int32_t status = inillucent_txn_execute((inillucent_txn *)(intptr_t)txn, text, &affected,
                                            &error);
    free(text);
    if (status != INILLUCENT_OK) {
        throw_failure(env, status, error);
        return 0;
    }
    return (jlong)affected;
}

/** Commits; the handle is spent either way. @param txn - the transaction */
JNIEXPORT void JNICALL NATIVE(txnCommit)(JNIEnv *env, jclass cls, jlong txn) {
    (void)cls;
    inillucent_error *error = NULL;
    int32_t status = inillucent_txn_commit((inillucent_txn *)(intptr_t)txn, &error);
    if (status != INILLUCENT_OK) throw_failure(env, status, error);
}

/** Rolls back and frees. @param txn - the transaction */
JNIEXPORT void JNICALL NATIVE(txnRollback)(JNIEnv *env, jclass cls, jlong txn) {
    (void)env; (void)cls;
    inillucent_txn_rollback((inillucent_txn *)(intptr_t)txn);
}
