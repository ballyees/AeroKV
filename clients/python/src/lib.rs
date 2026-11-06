#![allow(clippy::useless_conversion)]

use bytes::Bytes;
use core_client::AeroClient;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::sync::Arc;
use std::time::Duration;

#[pyclass]
struct Client {
    inner: Arc<AeroClient>,
}

#[pymethods]
impl Client {
    #[staticmethod]
    #[pyo3(signature = (host, port, token=None, prefix=None))]
    fn connect<'py>(
        py: Python<'py>,
        host: String,
        port: u16,
        token: Option<String>,
        prefix: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let client = AeroClient::connect_with_prefix(
                &host,
                port,
                token.as_deref(),
                prefix.as_deref(),
            )
            .await
            .map_err(|e| pyo3::exceptions::PyConnectionError::new_err(e.to_string()))?;
            Ok(Client { inner: client })
        })
    }

    fn with_prefix(&self, prefix: String) -> Client {
        Client {
            inner: self.inner.with_prefix(&prefix),
        }
    }

    fn prefix(&self) -> Option<String> {
        self.inner.prefix().map(|p| p.to_string())
    }

    fn protocol(&self) -> &'static str {
        self.inner.active_protocol()
    }

    fn get<'py>(&self, py: Python<'py>, key: String) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            match client.get(&key).await {
                Ok(Some(bytes)) => {
                    Python::with_gil(|py| Ok(Some(PyBytes::new_bound(py, &bytes).into_py(py))))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
            }
        })
    }

    #[pyo3(signature = (key, value, ttl_ms=None))]
    fn set<'py>(
        &self,
        py: Python<'py>,
        key: String,
        value: Vec<u8>,
        ttl_ms: Option<u64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        let bytes = Bytes::from(value);
        let ttl = ttl_ms.map(Duration::from_millis);

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            client
                .set(&key, bytes, ttl)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(true)
        })
    }

    #[pyo3(signature = (key, value, ttl_ms=None))]
    fn setnx<'py>(
        &self,
        py: Python<'py>,
        key: String,
        value: Vec<u8>,
        ttl_ms: Option<u64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        let bytes = Bytes::from(value);
        let ttl = ttl_ms.map(Duration::from_millis);

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let ok = client
                .setnx(&key, bytes, ttl)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(ok)
        })
    }

    fn delete<'py>(&self, py: Python<'py>, key: String) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let deleted = client
                .delete(&key)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(deleted)
        })
    }

    #[pyo3(signature = (key, delta=None))]
    fn incr<'py>(&self, py: Python<'py>, key: String, delta: Option<i64>) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        let d = delta.unwrap_or(1);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let val = client
                .incr(&key, d)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(val)
        })
    }

    fn ttl<'py>(&self, py: Python<'py>, key: String) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let val = client
                .ttl(&key)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(val)
        })
    }

    fn exists<'py>(&self, py: Python<'py>, key: String) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let ok = client
                .exists(&key)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(ok)
        })
    }

    fn ping<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            client
                .ping()
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(true)
        })
    }

    fn flush<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            client
                .flush()
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(true)
        })
    }

    fn publish<'py>(&self, py: Python<'py>, channel: String, message: Vec<u8>) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        let bytes = Bytes::from(message);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let count = client
                .publish(&channel, bytes)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(count)
        })
    }

    #[pyo3(signature = (channel, message, capacity=None, ttl_ms=None))]
    fn chan_send<'py>(
        &self,
        py: Python<'py>,
        channel: String,
        message: Vec<u8>,
        capacity: Option<usize>,
        ttl_ms: Option<u64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        let bytes = Bytes::from(message);
        let cap = capacity.unwrap_or(100);
        let ttl = ttl_ms.map(Duration::from_millis).unwrap_or(Duration::from_secs(60));
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let ok = client
                .chan_send(&channel, bytes, cap, ttl)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(ok)
        })
    }

    #[pyo3(signature = (channel, timeout_ms=None))]
    fn chan_receive<'py>(
        &self,
        py: Python<'py>,
        channel: String,
        timeout_ms: Option<u64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        let timeout = Duration::from_millis(timeout_ms.unwrap_or(30_000));
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            match client.chan_receive(&channel, timeout).await {
                Ok(Some(bytes)) => {
                    Python::with_gil(|py| Ok(Some(PyBytes::new_bound(py, &bytes).into_py(py))))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
            }
        })
    }

    fn group_add<'py>(&self, py: Python<'py>, group: String, channel: String) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            client
                .group_add(&group, &channel)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(true)
        })
    }

    fn group_discard<'py>(&self, py: Python<'py>, group: String, channel: String) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            client
                .group_discard(&group, &channel)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(true)
        })
    }

    #[pyo3(signature = (group, message, capacity=None, ttl_ms=None))]
    fn group_send<'py>(
        &self,
        py: Python<'py>,
        group: String,
        message: Vec<u8>,
        capacity: Option<usize>,
        ttl_ms: Option<u64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        let bytes = Bytes::from(message);
        let cap = capacity.unwrap_or(100);
        let ttl = ttl_ms.map(Duration::from_millis).unwrap_or(Duration::from_secs(60));
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let count = client
                .group_send(&group, bytes, cap, ttl)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(count)
        })
    }
}

#[pymodule]
fn _aerokv(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Client>()?;
    Ok(())
}
