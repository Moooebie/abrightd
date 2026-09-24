#include "abrightdcontroller.h"

#include <QDBusArgument>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusPendingCall>
#include <QDBusPendingCallWatcher>
#include <QDBusServiceWatcher>
#include <QDateTime>

namespace {
constexpr auto kService = "org.abrightd";
constexpr auto kPath = "/org/abrightd";
constexpr auto kInterface = "org.abrightd";
constexpr int kPollMs = 2000;
} // namespace

AbrightdController::AbrightdController(QObject *parent)
    : QObject(parent),
      m_iface(QString::fromLatin1(kService), QString::fromLatin1(kPath),
              QString::fromLatin1(kInterface), QDBusConnection::sessionBus()) {
    auto *watcher = new QDBusServiceWatcher(
        QString::fromLatin1(kService), QDBusConnection::sessionBus(),
        QDBusServiceWatcher::WatchForOwnerChange, this);
    connect(watcher, &QDBusServiceWatcher::serviceRegistered, this,
            [this] { refresh(); });
    connect(watcher, &QDBusServiceWatcher::serviceUnregistered, this,
            [this] { setAvailable(false); });

    m_timer.setInterval(kPollMs);
    connect(&m_timer, &QTimer::timeout, this, &AbrightdController::refresh);
    m_timer.start();

    refresh();
}

void AbrightdController::setEnabled(bool value) {
    if (!m_iface.isValid()) {
        refresh();
        return;
    }
    // Optimistically adopt the requested value and ignore poll results until
    // the daemon confirms it, so the switch does not flash back.
    m_pending = true;
    m_pendingValue = value;
    m_pendingUntilMs = QDateTime::currentMSecsSinceEpoch() + 3000;

    const QDBusPendingCall pending = m_iface.asyncCall(QStringLiteral("Enable"), value);
    auto *watcher = new QDBusPendingCallWatcher(pending, this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this,
            [this](QDBusPendingCallWatcher *call) {
                call->deleteLater();
                if (call->isError()) {
                    m_pending = false;
                    refresh();
                }
            });

    if (value != m_enabled) {
        m_enabled = value;
        Q_EMIT enabledChanged();
    }
}

void AbrightdController::refresh() {
    const QDBusMessage msg = m_iface.call(QStringLiteral("Status"));
    if (msg.type() == QDBusMessage::ErrorMessage || msg.arguments().isEmpty()) {
        setAvailable(false);
        return;
    }
    // `Status` returns `a{ss}`; QDBusReply/QVariantMap cannot demarshal that,
    // so cast the raw argument explicitly.
    const QMap<QString, QString> map =
        qdbus_cast<QMap<QString, QString>>(msg.arguments().at(0));
    if (map.isEmpty()) {
        setAvailable(false);
        return;
    }
    setAvailable(true);
    applyStatus(map);
}

void AbrightdController::applyStatus(const QMap<QString, QString> &status) {
    bool readingsChanged = false;

    const bool enabled =
        status.value(QStringLiteral("enabled")) == QLatin1String("true");

    // Ignore stale `enabled` values while a requested change is in flight.
    if (m_pending) {
        if (enabled == m_pendingValue) {
            m_pending = false; // daemon confirmed the change
        } else if (QDateTime::currentMSecsSinceEpoch() >= m_pendingUntilMs) {
            m_pending = false; // window elapsed; trust the daemon
        }
    }
    if (!m_pending && enabled != m_enabled) {
        m_enabled = enabled;
        Q_EMIT enabledChanged();
    }

    const auto readNumber = [&status](const char *key, double &out) {
        bool ok = false;
        const double value =
            status.value(QString::fromLatin1(key)).toDouble(&ok);
        if (ok && value != out) {
            out = value;
            return true;
        }
        return false;
    };

    readingsChanged |= readNumber("lux", m_lux);
    readingsChanged |= readNumber("output_brightness", m_brightness);
    readingsChanged |= readNumber("adjustment", m_adjustment);

    if (readingsChanged) {
        Q_EMIT statusChanged();
    }
}

void AbrightdController::setAvailable(bool value) {
    if (value == m_available) {
        return;
    }
    m_available = value;
    Q_EMIT availableChanged();
}
