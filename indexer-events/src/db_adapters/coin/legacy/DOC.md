## Legacy contracts

The contract is identified as legacy if it requires custom handling of its activity.
The most popular example: the contract has a period with some activity (coins minted/transferred/burnt), but no events were produced.
Another example: events are produced, but the part of the logic is not covered by events.  
We decided to make the custom support for some popular contracts.

If you are the author of the contract which does not produce the events, please upgrade your contract ASAP!
The old history (without corresponding events) will not be collected, but all the data starting from your upgrade wil be fetched successfully.

If it's important to collect the history when no events were produced, you have to write your own custom handler.
Please feel free to add implementation for your contract, we will re-index the data from time to time.

### Important details

The code here is duplicated more than it could be, but I decided not to generalize the logic because it's not intended to be reused or rewritten.  
I strongly recommend you to have a look at existing handlers before implementing your own one.

#### Mint

Some contracts may mint coins at `new` method: see `tkn_near`.  
Some contracts may mint coins at `ft_on_transfer` method: see `wentokensir`.

General mint methods are `mint`, `near_deposit`. The logic inside could differ a little.

#### Transfer
All the legacy contracts have the same logic regarding TRANSFER except Aurora.  
They implement `ft_transfer`, `ft_transfer_call`, `ft_resolve_transfer`, the code for handling transfers is the same except Aurora.

#### Burn

Most of the contracts have burn logic: see `withdraw`, `near_withdraw`. The logic inside also differs a little.
