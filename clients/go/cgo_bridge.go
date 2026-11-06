package aerokv

/*
#cgo LDFLAGS: -L${SRCDIR}/../../target/release -L${SRCDIR}/../../target/debug -lcore_client
#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>

typedef void* AeroClientPtr;

extern AeroClientPtr aerokv_client_new(const char* host, uint16_t port, const char* token);
extern AeroClientPtr aerokv_client_new_with_prefix(const char* host, uint16_t port, const char* token, const char* prefix);
extern void aerokv_client_free(AeroClientPtr client);
extern uint8_t* aerokv_client_get(AeroClientPtr client, const char* key, size_t* out_len);
extern bool aerokv_client_set(AeroClientPtr client, const char* key, const uint8_t* val, size_t val_len, uint64_t ttl_ms);
extern bool aerokv_client_delete(AeroClientPtr client, const char* key);
extern bool aerokv_client_incr(AeroClientPtr client, const char* key, int64_t delta, int64_t* out_val);
extern void aerokv_bytes_free(uint8_t* ptr, size_t len);
*/
import "C"
import (
	"errors"
	"unsafe"
)

type Client struct {
	ptr C.AeroClientPtr
}

// NewClient creates a new AeroKV client using the Rust core-client FFI
func NewClient(host string, port uint16, token string) (*Client, error) {
	return NewClientWithPrefix(host, port, token, "")
}

// NewClientWithPrefix creates a new AeroKV client with an optional key prefix
func NewClientWithPrefix(host string, port uint16, token string, prefix string) (*Client, error) {
	cHost := C.CString(host)
	defer C.free(unsafe.Pointer(cHost))

	var cToken *C.char
	if token != "" {
		cToken = C.CString(token)
		defer C.free(unsafe.Pointer(cToken))
	}

	var cPrefix *C.char
	if prefix != "" {
		cPrefix = C.CString(prefix)
		defer C.free(unsafe.Pointer(cPrefix))
	}

	ptr := C.aerokv_client_new_with_prefix(cHost, C.uint16_t(port), cToken, cPrefix)
	if ptr == nil {
		return nil, errors.New("failed to initialize AeroKV client")
	}

	return &Client{ptr: ptr}, nil
}

// Get fetches a value by key
func (c *Client) Get(key string) ([]byte, error) {
	if c.ptr == nil {
		return nil, errors.New("client closed")
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))

	var outLen C.size_t
	bytesPtr := C.aerokv_client_get(c.ptr, cKey, &outLen)
	if bytesPtr == nil {
		return nil, nil // Key not found
	}
	defer C.aerokv_bytes_free(bytesPtr, outLen)

	return C.GoBytes(unsafe.Pointer(bytesPtr), C.int(outLen)), nil
}

// Set stores a key-value pair with optional TTL in milliseconds
func (c *Client) Set(key string, val []byte, ttlMs uint64) error {
	if c.ptr == nil {
		return errors.New("client closed")
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))

	var valPtr *C.uint8_t
	if len(val) > 0 {
		valPtr = (*C.uint8_t)(unsafe.Pointer(&val[0]))
	}

	ok := C.aerokv_client_set(c.ptr, cKey, valPtr, C.size_t(len(val)), C.uint64_t(ttlMs))
	if !ok {
		return errors.New("set operation failed")
	}
	return nil
}

// Delete removes a key
func (c *Client) Delete(key string) (bool, error) {
	if c.ptr == nil {
		return false, errors.New("client closed")
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))

	ok := C.aerokv_client_delete(c.ptr, cKey)
	return bool(ok), nil
}

// Incr increments or decrements an integer value atomically
func (c *Client) Incr(key string, delta int64) (int64, error) {
	if c.ptr == nil {
		return 0, errors.New("client closed")
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))

	var outVal C.int64_t
	ok := C.aerokv_client_incr(c.ptr, cKey, C.int64_t(delta), &outVal)
	if !ok {
		return 0, errors.New("incr operation failed")
	}
	return int64(outVal), nil
}

// Close frees the client resources
func (c *Client) Close() {
	if c.ptr != nil {
		C.aerokv_client_free(c.ptr)
		c.ptr = nil
	}
}
