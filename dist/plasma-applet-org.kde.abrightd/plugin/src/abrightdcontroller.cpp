#include "abrightdcontroller.h"

#include <QDBusArgument>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusPendingCall>
#include <QDBusPendingCallWatcher>
#include <QDBusServiceWatcher>
#include <QDateTime>
#include <QtGlobal>

namespace {
constexpr auto kService = "org.abrightd";
constexpr auto kPath = "/org/abrightd";
constexpr auto kInterface = "org.abrightd";
constexpr int kPollMs = 2000;
constexpr qint64 kEnabledPendingMs = 3000;
constexpr qint64 kAdjustmentPendingMs = 1500;
constexpr double kAdjustmentEpsilon = 1e-4;
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
    m_enabledPending = true;
    m_pendingEnabledValue = value;
    m_enabledPendingUntilMs =
        QDateTime::currentMSecsSinceEpoch() + kEnabledPendingMs;
    if (value != m_enabled) {
        m_enabled = value;
        Q_EMIT enabledChanged();
    }

    if (!m_iface.isValid()) {
        refresh();
        return;
    }
    const QDBusPendingCall pending = m_iface.asyncCall(QStringLiteral("Enable"), value);
    auto *watcher = new QDBusPendingCallWatcher(pending, this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this,
            [this](QDBusPendingCallWatcher *call) {
                call->deleteLater();
                if (call->isError()) {
                    m_enabledPending = false;
                    refresh();
                }
            });
}

void AbrightdController::setAdjustment(double value) {
    value = qBound(-1.0, value, 1.0);
    m_adjustmentPending = true;
    m_pendingAdjustment = value;
    m_adjustmentPendingUntilMs =
        QDateTime::currentMSecsSinceEpoch() + kAdjustmentPendingMs;
    if (qAbs(value - m_adjustment) > kAdjustmentEpsilon) {
        m_adjustment = value;
        Q_EMIT adjustmentChanged();
    }

    if (!m_iface.isValid()) {
        refresh();
        return;
    }
    const QDBusPendingCall pending =
        m_iface.asyncCall(QStringLiteral("SetAdjustment"), value);
    auto *watcher = new QDBusPendingCallWatcher(pending, this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this,
            [this](QDBusPendingCallWatcher *call) {
                call->deleteLater();
                if (call->isError()) {
                    m_adjustmentPending = false;
                    refresh();
                }
            });
}

void AbrightdController::resetCalibration() {
    m_adjustmentPending = true;
    m_pendingAdjustment = 0.0;
    m_adjustmentPendingUntilMs =
        QDateTime::currentMSecsSinceEpoch() + kAdjustmentPendingMs;
    if (qAbs(m_adjustment) > kAdjustmentEpsilon) {
        m_adjustment = 0.0;
        Q_EMIT adjustmentChanged();
    }
    if (m_pointCalibrated) {
        m_pointCalibrated = false;
        Q_EMIT pointCalibratedChanged();
    }

    if (!m_iface.isValid()) {
        refresh();
        return;
    }
    const QDBusPendingCall pending =
        m_iface.asyncCall(QStringLiteral("ResetCalibration"));
    auto *watcher = new QDBusPendingCallWatcher(pending, this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this,
            [this](QDBusPendingCallWatcher *call) {
                call->deleteLater();
                if (call->isError()) {
                    m_adjustmentPending = false;
                    refresh();
                }
            });
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
    const qint64 now = QDateTime::currentMSecsSinceEpoch();

    // enabled (ignore stale values while a change is in flight)
    const bool enabled =
        status.value(QStringLiteral("enabled")) == QLatin1String("true");
    if (m_enabledPending) {
        if (enabled == m_pendingEnabledValue) {
            m_enabledPending = false;
        } else if (now >= m_enabledPendingUntilMs) {
            m_enabledPending = false;
        }
    }
    if (!m_enabledPending && enabled != m_enabled) {
        m_enabled = enabled;
        Q_EMIT enabledChanged();
    }

    // adjustment (same optimistic guard)
    bool ok = false;
    const double adjustment = status.value(QStringLiteral("adjustment")).toDouble(&ok);
    if (ok) {
        if (m_adjustmentPending) {
            if (qAbs(adjustment - m_pendingAdjustment) < kAdjustmentEpsilon) {
                m_adjustmentPending = false;
            } else if (now >= m_adjustmentPendingUntilMs) {
                m_adjustmentPending = false;
            }
        }
        if (!m_adjustmentPending &&
            qAbs(adjustment - m_adjustment) > kAdjustmentEpsilon) {
            m_adjustment = adjustment;
            Q_EMIT adjustmentChanged();
        }
    }

    // point-calibration indicator
    const bool point = !status.value(QStringLiteral("user_points")).isEmpty();
    if (point != m_pointCalibrated) {
        m_pointCalibrated = point;
        Q_EMIT pointCalibratedChanged();
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
