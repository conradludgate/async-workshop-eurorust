# Tangent - Channel shutdown

Before we cover how to write an async channel, let's first cover the edge-cases we will need to consider.

## Closing the receiver

It's possible that we have dropped the receiver of a channel while still having some
of the senders alive. Ideally they will gracefully handle when there are no tasks
that will possibly listen to the message, and not bother wasting memory.
Perhaps also returning a value to indicate that it was closed.

## Closing the senders

It's also possible that we have dropped all the senders of a channel while the receiver is still alive.

We could get lucky and we might not currently be waiting on a value. When we finally try to receive a value,
we could check if there are any senders.

However, another edge case that shows up is when we are currently waiting for a value from the channel, and the
last sender is dropped. In this case we would somehow need to notify the channel receiver that it is time to give up the waiting.
