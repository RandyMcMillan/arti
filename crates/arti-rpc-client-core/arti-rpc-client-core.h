/**
 * # Arti RPC core library header.
 *
 * (TODO RPC: This is still a work in progress; please don't rely on it
 * being the final API.)
 *
 * ## What this library does
 *
 * The Arti RPC system works by establishing connections to an Arti instance,
 * and then exchanging requests and replies in a format inspired by
 * JSON-RPC.  This library takes care of the work of connecting to an Arti
 * instance, authenticating, validating outgoing JSON requests, and matching
 * their corresponding JSON responses as they arrive.
 *
 * This library _does not_ do the work of creating well-formed requests,
 * interpreting the responses, or exposing these pairs.
 *
 * (Note: Despite this library being exposed via a set of C functions,
 * we don't actually expect you to use it from C.  It's probably a better
 * idea to wrap it in a higher-level language and then use it from there.)
 *
 * ## Using this library
 *
 * TODO RPC Explain better.
 *
 * Your connection to Arti is represented by an `ArtiRpcConn`.  Use
 * `arti_connect()` to create one of these.
 *
 * Once you have a connection, you can sent Arti various requests in
 * JSON format.  See (TODO RPC: Add a link to a list of comments.)
 * Use `arti_rpc_execute()` to send a simple request; the function will
 * return when the request succeeds, or fails.
 *
 * TODO: Explain handles and other APIs once I add those APIs.
 *
 * Except when noted otherwise, all functions in this library are thread-safe.
 *
 * ## Error handling
 *
 * On success, fallible functions return `ARTI_SUCCESS`.  On failure,
 * they return some other error code, and store the most recent
 * error in a thread-local variable.
 *
 * You can access information about the most recent error
 * by calling `arti_err_{status,message,response}(NULL)`.
 * Alternatively, you can make a copy of the most recent error
 * by calling `arti_err_clone(NULL)`, and then passing the resulting error
 * to the `arti_err_{status,message,response}()` functions.
 *
 * ## Interface conventions
 *
 * - All functions check for NULL pointers in their arguments.
 *   - As in C tor, `foo_free()` functions treat `foo_free(NULL)` as a no-op
 *
 * - Fallible functions return an ArtiStatus.
 *
 * - All identifiers are prefixed with `arti`, in some case.
 *
 * ## Safety
 *
 * - Basic C safety requirements apply: every function's input pointers
 *   must point to valid data of the correct type.
 * - All input objects must not be mutated while they are in use.
 * - All input strings must obey the additional requirements of CStr::from_ptr:
 *   They must be valid for their entire extent, they must be no larger than SSIZE_MAX,
 *   they must be nul-terminated, and they must not be mutated while in use.
 **/

#ifndef ARTI_RPC_CLIENT_CORE_H_
#define ARTI_RPC_CLIENT_CORE_H_

/* Automatically generated by cbindgen. Don't modify manually. */

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * A status code returned by an Arti RPC function.
 *
 * On success, a function will return `ARTI_SUCCESS (0)`.
 * On failure, a function will return some other status code.
 */
typedef uint32_t ArtiStatus;

/**
 * An open connection to Arti over an a RPC protocol.
 *
 * This is a thread-safe type: you may safely use it from multiple threads at once.
 *
 * Once you are no longer going to use this connection at all, you must free
 * it with [`arti_rpc_conn_free`]
 */
typedef struct ArtiRpcConn ArtiRpcConn;

/**
 * An error returned by the Arti RPC code, exposed as an object.
 *
 * After a function has returned an [`ArtiStatus`] other than [`ARTI_SUCCESS`],
 * you can use [`arti_err_clone`]`(NULL)` to get a copy of the most recent error.
 *
 * Functions that return information about an error will either take a pointer
 * to one of these objects, or NULL to indicate the most error in a given thread.
 */
typedef struct ArtiError ArtiError;

/**
 * The function has returned successfully.
 */
#define ARTI_SUCCESS 0

/**
 * One or more of the inputs to the function was invalid.
 */
#define ARTI_INVALID_INPUT 1

/**
 * Tried to use some functionality (for example, an authentication method or connection scheme)
 * that wasn't available on this platform or build.
 */
#define ARTI_NOT_SUPPORTED 2

/**
 * Tried to connect to Arti, but an IO error occurred.
 */
#define ARTI_CONNECT_IO 3

/**
 * We tried to authenticate with Arti, but it rejected our attempt.
 */
#define ARTI_BAD_AUTH 4

/**
 * Our peer has, in some way, violated the Arti-RPC protocol.
 */
#define ARTI_PEER_PROTOCOL_VIOLATION 5

/**
 * The peer has closed our connection; possibly because it is shutting down.
 */
#define ARTI_SHUTDOWN 6

/**
 * An internal error occurred in the arti rpc client.
 */
#define ARTI_INTERNAL 7

/**
 * The peer reports that one of our requests has failed.
 */
#define ARTI_REQUEST_FAILED 8

/**
 * Tried to check the status of a request and found that it was no longer running.
 *
 * TODO RPC: We should make sure that this is the actual semantics we want for this
 * error!  Revisit after we have implemented real cancellation.
 */
#define ARTI_REQUEST_CANCELLED 9













#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Try to open a new connection to an Arti instance.
 *
 * The location of the instance and the method to connect to it are described in
 * `connection_string`.
 *
 * On success, return `ARTI_SUCCESS` and set `*rpc_conn_out` to a new ArtiRpcConn.
 * Otherwise returns some other status cod and set `*rpc_conn_out` to NULL.
 *
 * # Safety
 *
 * Standard safety warnings apply; see library header.
 */
ArtiStatus arti_connect(const char *connection_string,
                        ArtiRpcConn **rpc_conn_out);

/**
 * Run an RPC request over `rpc_conn` and wait for a successful response.
 *
 * The message `msg` should be a valid RPC request in JSON format.
 * If you omit its `id`` field, one will be generated: this is typically the best way to use this function.
 *
 * On success, return `ARTI_SUCCESS` and set `*response_out` to a newly allocated string
 * containing the Json response to your request (including `id` and `response` fields).
 *
 * Otherwise returns some other status code, and set `*response_out` to NULL.
 *
 * (If response_out is set to NULL, then any successful response will be ignored.)
 *
 * # Safety
 *
 * The caller must not modify the length of `*response_out`.
 *
 * The caller must free `*response_out` with `arti_free_str()`, not with `free()` or any other call.
 */
ArtiStatus arti_rpc_execute(const ArtiRpcConn *rpc_conn,
                            const char *msg,
                            char **response_out);

/**
 * Free a string returned by the Arti RPC API.
 *
 * # Safety
 *
 * The string must be returned by the Arti RPC API.
 *
 * The string must not have been modified since it was returned.
 *
 * After you have called this function, it is not safe to use the provided pointer from any thread.
 */
void arti_free_str(char *string);

/**
 * Close and free an open Arti RPC connection.
 *
 * # Safety
 *
 * After you have called this function, it is not safe to use the provided pointer from any thread.
 */
void arti_rpc_conn_free(ArtiRpcConn *rpc_conn);

/**
 * Return a string representing the meaning of a given `arti_status_t`.
 *
 * The result will always be non-NULL, even if the status is unrecognized.
 */
const char *arti_status_to_str(ArtiStatus status);

/**
 * Return the status code associated with a given error.
 *
 * If `err` is NULL, instead return the status code from the most recent error to occur in this
 * thread.
 *
 * # Safety
 *
 * The provided pointer, if non-NULL, must be a valid `ArtiError`.
 */
ArtiStatus arti_err_status(const ArtiError *err);

/**
 * Return a human-readable error message associated with a given error.
 *
 * If `err` is NULL, instead return the error message from the most recent error to occur in this
 * thread.
 *
 * The format of these messages may change arbitrarily between versions of this library;
 * it is a mistake to depend on the actual contents of this message.
 *
 * # Safety
 *
 * The returned pointer is only as valid for as long as `err` is valid.
 *
 * If `err` is NULL, then the returned pointer is only valid until another
 * error occurs in this thread.
 */
const char *arti_err_message(const ArtiError *err);

/**
 * Return a Json-formatted error response associated with a given error.
 *
 * If `err` is NULL, instead return the response from the most recent error to occur in this
 * thread.
 *
 * These messages are full responses, including the `error` field,
 * and the `id` field (if present).
 *
 * Return NULL if the specified error does not represent an RPC error response.
 *
 * # Safety
 *
 * The returned pointer is only as valid for as long as `err` is valid.
 *
 * If `err` is NULL, then the returned pointer is only valid until another
 * error occurs in this thread.
 */
const char *arti_err_response(const ArtiError *err);

/**
 * Make and return copy of a provided error.
 *
 * If `err` is NULL, instead return a copy of the most recent error to occur in this thread.
 *
 * May return NULL if an internal error occurs.
 *
 * # Safety
 *
 * The resulting error may only be freed via `arti_err_free().`
 */
ArtiError *arti_err_clone(const ArtiError *err);

/**
 * Release storage held by a provided error.
 *
 * # Safety
 *
 * The provided pointer must be returned by `arti_err_clone`.
 * After this call, it may not longer be used.
 */
void arti_err_free(ArtiError *err);

#ifdef __cplusplus
} // extern "C"
#endif // __cplusplus

#endif /* ARTI_RPC_CLIENT_CORE_H_ */
