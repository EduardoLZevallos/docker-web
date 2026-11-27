To make it worth using, focus on:
- A clear graph of networks, attached containers, IPs, and published ports.
- Useful troubleshooting: containers on the wrong network, overlapping subnets, and unexpected port exposure.
- Easy deployment and a read-only interface.
One important distinction: sharing a Docker network doesn’t prove two containers actually communicate. Label the graph as network membership; actual traffic mapping would be a separate feature.
I’d build the small topology viewer first and validate it with Docker users. That’s a credible niche, though the earlier competitor list alone doesn’t establish demand or that the space is currently empty.
