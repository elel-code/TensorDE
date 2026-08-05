use std::{future::Future, marker::PhantomData, pin::Pin};

use serde::{Serialize, de::DeserializeOwned};
use zvariant::{DynamicType, Type};

use crate::{
    Connection, ConnectionMode, MethodCall, MethodResult, PeerCredentials, Result,
    reply_method_error, reply_method_result,
};

pub(super) type HandlerFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + 'a>>;

pub(super) trait ErasedHandler {
    fn call<'a>(
        &'a mut self,
        connection: &'a mut Connection,
        call: &'a MethodCall,
    ) -> HandlerFuture<'a>;
}

pub(super) struct TypedHandler<H, A, R, F> {
    pub(super) handler: H,
    pub(super) marker: PhantomData<fn(A) -> (R, F)>,
}

pub(super) struct ContextHandler<H, A, R, F> {
    pub(super) handler: H,
    pub(super) marker: PhantomData<fn(A) -> (R, F)>,
}

pub(super) struct ConnectionHandler<H, A, R> {
    pub(super) handler: H,
    pub(super) marker: PhantomData<fn(A) -> R>,
}

/// Authenticated connection metadata captured for a contextual method call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodContext {
    mode: ConnectionMode,
    peer_credentials: Option<PeerCredentials>,
    sender: Option<String>,
}

impl MethodContext {
    pub const fn mode(&self) -> ConnectionMode {
        self.mode
    }

    pub const fn peer_credentials(&self) -> Option<PeerCredentials> {
        self.peer_credentials
    }

    pub fn sender(&self) -> Option<&str> {
        self.sender.as_deref()
    }
}

impl<H, A, R, F> ErasedHandler for TypedHandler<H, A, R, F>
where
    H: FnMut(A) -> F,
    A: DeserializeOwned + Type,
    R: Serialize + DynamicType,
    F: Future<Output = MethodResult<R>> + 'static,
{
    fn call<'a>(
        &'a mut self,
        connection: &'a mut Connection,
        call: &'a MethodCall,
    ) -> HandlerFuture<'a> {
        let arguments = call.body::<A>();
        match arguments {
            Ok(arguments) => {
                let future = (self.handler)(arguments);
                Box::pin(async move { reply_method_result(connection, call, future.await).await })
            }
            Err(error) => Box::pin(reply_method_error(connection, call, error)),
        }
    }
}

impl<H, A, R, F> ErasedHandler for ContextHandler<H, A, R, F>
where
    H: FnMut(MethodContext, A) -> F,
    A: DeserializeOwned + Type,
    R: Serialize + DynamicType,
    F: Future<Output = MethodResult<R>> + 'static,
{
    fn call<'a>(
        &'a mut self,
        connection: &'a mut Connection,
        call: &'a MethodCall,
    ) -> HandlerFuture<'a> {
        let context = MethodContext {
            mode: connection.mode(),
            peer_credentials: connection.peer_credentials(),
            sender: call.sender().map(str::to_owned),
        };
        let arguments = call.body::<A>();
        match arguments {
            Ok(arguments) => {
                let future = (self.handler)(context, arguments);
                Box::pin(async move { reply_method_result(connection, call, future.await).await })
            }
            Err(error) => Box::pin(reply_method_error(connection, call, error)),
        }
    }
}

impl<H, A, R> ErasedHandler for ConnectionHandler<H, A, R>
where
    H: for<'a> AsyncFnMut(&'a mut Connection, MethodContext, A) -> MethodResult<R>,
    A: DeserializeOwned + Type,
    R: Serialize + DynamicType,
{
    fn call<'a>(
        &'a mut self,
        connection: &'a mut Connection,
        call: &'a MethodCall,
    ) -> HandlerFuture<'a> {
        let context = MethodContext {
            mode: connection.mode(),
            peer_credentials: connection.peer_credentials(),
            sender: call.sender().map(str::to_owned),
        };
        let arguments = call.body::<A>();
        match arguments {
            Ok(arguments) => Box::pin(async move {
                let result = (self.handler)(connection, context, arguments).await;
                reply_method_result(connection, call, result).await
            }),
            Err(error) => Box::pin(reply_method_error(connection, call, error)),
        }
    }
}
