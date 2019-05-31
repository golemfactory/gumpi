FROM rust as build
ARG GU_BRANCH=${GU_BRANCH:-release/0.2}
RUN git clone git://github.com/golemfactory/golem-unlimited.git
RUN cd golem-unlimited && git fetch && git checkout $GU_BRANCH
RUN cargo install --path golem-unlimited/gu-provider

FROM rastasheep/ubuntu-sshd:18.04
ENV USER mpirun

ENV HOME=/home/${USER}

RUN apt-get update -y && apt-get -y upgrade && apt-get install -y software-properties-common && \
    add-apt-repository -y ppa:marmistrz/openmpi && \
    apt-get install -y curl cmake git autoconf build-essential gcc-8 g++-8 gfortran-8 libopenmpi-dev openmpi-bin openmpi-common htop && \
    update-alternatives --install /usr/bin/gcc gcc /usr/bin/gcc-8 60 --slave /usr/bin/g++ g++ /usr/bin/g++-8 && \
    update-alternatives --config gcc && \
    curl https://sh.rustup.rs -sSf | sh -s -- -y && \
    apt-get clean && apt-get purge && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*

# ------------------------------------------------------------
# Add an 'mpirun' user
# ------------------------------------------------------------

RUN adduser --disabled-password --gecos "" ${USER} && \
    echo "${USER} ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# ------------------------------------------------------------
# Harden the SSHD configuration
# ------------------------------------------------------------

RUN sed -i 's/\#Port 22/Port 4222/' /etc/ssh/sshd_config && \
    sed -i 's/\#PasswordAuthentication yes/PasswordAuthentication no/' /etc/ssh/sshd_config && \
    sed -i 's/\#UsePAM yes/UsePAM no/' /etc/ssh/sshd_config

# ------------------------------------------------------------
# Set-Up SSH with our Github deploy key
# ------------------------------------------------------------

ENV SSHDIR ${HOME}/.ssh/

RUN mkdir -p ${SSHDIR}

RUN echo "StrictHostKeyChecking no" > ${SSHDIR}/config
ADD ssh/mpi ${SSHDIR}/id_rsa
ADD ssh/mpi.pub ${SSHDIR}/id_rsa.pub
ADD ssh/mpi.pub ${SSHDIR}/authorized_keys

RUN chmod -R 600 ${SSHDIR}* && \
    chown -R ${USER}:${USER} ${SSHDIR} && \
    chown -R ${USER}:${USER} ${HOME}/.cargo && \
    chown ${USER}:${USER} ${HOME}
#USER $USER

COPY --from=build /usr/local/cargo/bin/gu-provider /usr/bin/gu-provider
COPY init.sh /usr/bin/
WORKDIR /opt/provider
ENV RUST_LOG=info
CMD /usr/bin/init.sh
