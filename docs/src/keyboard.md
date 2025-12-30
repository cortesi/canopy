# Keyboard

<img width=300 style="padding: 20px;" src="assets/keyevent.png">

Key events are passed down from the current focus to the root, with the
**Node::handle_key** method called on each node. Keys are only handled once - we
stop passing the event along once the first node indicates that it's been
handled. Rendering occurs after each event; **EventOutcome** only controls
propagation.
