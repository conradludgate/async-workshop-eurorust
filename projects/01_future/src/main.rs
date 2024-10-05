use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

#[pin_project::pin_project]
struct Select<F1, F2> {
    #[pin]
    left: F1,
    #[pin]
    right: F2,
}

#[derive(Debug)]
enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<F1: Future, F2: Future> Future for Select<F1, F2> {
    type Output = Either<F1::Output, F2::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        if let Poll::Ready(left) = this.left.poll(cx) {
            return Poll::Ready(Either::Left(left));
        }

        if let Poll::Ready(right) = this.right.poll(cx) {
            return Poll::Ready(Either::Right(right));
        }

        Poll::Pending
    }
}

#[tokio::main]
async fn main() {
    let (tx, rx) = tokio::sync::oneshot::channel();

    tokio::task::spawn(async {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let _ = tx.send(());
    });

    let res = Select {
        left: tokio::time::sleep(Duration::from_secs(3)),
        right: rx,
    };

    println!("raced: {:?}", res.await);
}
