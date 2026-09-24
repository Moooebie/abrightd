#include "abrightdcontroller.h"

#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusReply>
#include <QDBusServiceWatcher>

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
    // Always call through: the local state may be stale, and Enable is
    // idempotent, so re-issuing is harmless.
    const QDBusMessage reply = m_iface.call(QStringLiteral("Enable"), value);
    if (reply.type() == QDBusMessage::ErrorMessage) {
        refresh();
        return;
    }
    if (value != m_enabled) {
        m_enabled = value;
        Q_EMIT enabledChanged();
    }
}

void AbrightdController::refresh() {
    if (!m_iface.isValid()) {
        setAvailable(false);
        return;
    }
    const QDBusReply<QVariantMap> reply =
        m_iface.call(QStringLiteral("Status"));
    if (!reply.isValid()) {
        setAvailable(false);
        return;
    }
    setAvailable(true);
    applyStatus(reply.value());
}

void AbrightdController::applyStatus(const QVariantMap &status) {
    bool readingsChanged = false;

    const bool enabled =
        status.value(QStringLiteral("enabled")).toString() == QLatin1String("true");
    if (enabled != m_enabled) {
        m_enabled = enabled;
        Q_EMIT enabledChanged();
    }

    const auto readNumber = [&status](const char *key, double &out) {
        bool ok = false;
        const double value =
            status.value(QString::fromLatin1(key)).toString().toDouble(&ok);
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
