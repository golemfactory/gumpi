FROM rust as build
ARG GU_BRANCH=${GU_BRANCH:-feature/blender}
RUN git clone git://github.com/golemfactory/golem-unlimited.git 
RUN cd golem-unlimited && git fetch && git checkout $GU_BRANCH
RUN cargo install --path golem-unlimited/gu-hub 
RUN mkdir /src
COPY . /src
RUN cargo install --path /src
RUN git clone https://github.com/marmistrz/game-life.git && \
	cd /game-life && \
	mkdir -p /examples && \
	git archive --format tar -o /examples/game-life.tar master && \
	cp /src/examples/game-life.toml /examples

FROM ubuntu
RUN mkdir -p /opt/hub
COPY --from=build /usr/local/cargo/bin/gu-hub /usr/bin/gu-hub
COPY --from=build /usr/local/cargo/bin/gumpi /usr/bin/gumpi
COPY --from=build /golem-unlimited/gu-hub/webapp /opt/hub/webapp/
COPY --from=build /examples /examples
WORKDIR /opt/hub
ENV RUST_LOG=info
EXPOSE 61622
ENTRYPOINT [ "/usr/bin/gu-hub" ]
CMD [ "server", "run" ]


