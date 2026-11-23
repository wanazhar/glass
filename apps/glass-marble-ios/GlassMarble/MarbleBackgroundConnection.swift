import ActivityKit
import CoreLocation
import OSLog

/// Owns the low-accuracy session and the Live Activity for the current SSE
/// connection. Location values are discarded at the delegate boundary.
@MainActor
final class MarbleBackgroundConnection: NSObject, @preconcurrency CLLocationManagerDelegate {
    private static let log = Logger(subsystem: "com.wanazhar.glassmarble", category: "background")
    private lazy var location: CLLocationManager = {
        let manager = CLLocationManager()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyThreeKilometers
        manager.distanceFilter = 3_000
        manager.showsBackgroundLocationIndicator = true
        manager.pausesLocationUpdatesAutomatically = false
        return manager
    }()

    private var wantsLocation = false
    private var requestedAuthorization = false
    private var activity: Activity<MarbleConnectionActivityAttributes>?
    private var activityHeartbeat: Task<Void, Never>?
    private var activityConnectedAt: Date?
    private(set) var isKeepingAlive = false
    var authorizationChanged: (() -> Void)?

    var permissionDenied: Bool {
        location.authorizationStatus == .denied || location.authorizationStatus == .restricted
    }

    func setLocationNeeded(_ needed: Bool, canStart: Bool) {
        wantsLocation = needed
        if needed {
            guard canStart else { return }
            switch location.authorizationStatus {
            case .notDetermined:
                guard !requestedAuthorization else { return }
                requestedAuthorization = true
                location.requestWhenInUseAuthorization()
            case .authorizedWhenInUse, .authorizedAlways:
                guard !isKeepingAlive else { return }
                location.allowsBackgroundLocationUpdates = true
                location.startUpdatingLocation()
                isKeepingAlive = true
            default:
                break
            }
        } else if isKeepingAlive {
            stopLocation()
        }
    }

    private func stopLocation() {
        location.stopUpdatingLocation()
        location.allowsBackgroundLocationUpdates = false
        isKeepingAlive = false
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        if isKeepingAlive && permissionDenied { stopLocation() }
        if wantsLocation { authorizationChanged?() }
    }

    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {}

    func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) {}

    /// ActivityKit requires a foreground request, so use the scene's
    /// will-resign callback before iOS completes the background transition.
    func startActivity(connectedAt: Date, host: String) -> Bool {
        guard isKeepingAlive, activity == nil, ActivityAuthorizationInfo().areActivitiesEnabled else { return false }
        do {
            activity = try Activity.request(
                attributes: .init(host: host),
                content: activityContent(connectedAt: connectedAt),
                pushType: nil
            )
            activityConnectedAt = connectedAt
            startActivityHeartbeat()
            return true
        } catch {
            Self.log.debug("Could not start connection Live Activity: \(error.localizedDescription, privacy: .public)")
            return false
        }
    }

    func showReconnecting() {
        refreshActivity(connectedAt: nil)
    }

    func refreshActivity(connectedAt: Date?) {
        guard let activity, connectedAt != activityConnectedAt else { return }
        activityConnectedAt = connectedAt
        let content = activityContent(connectedAt: connectedAt)
        Task { await activity.update(content) }
    }

    private func startActivityHeartbeat() {
        activityHeartbeat = Task { [weak self] in
            while !Task.isCancelled {
                do { try await Task.sleep(for: .seconds(10)) } catch { return }
                guard let self, let activity = self.activity else { return }
                await activity.update(self.activityContent(connectedAt: self.activityConnectedAt))
            }
        }
    }

    private func activityContent(connectedAt: Date?) -> ActivityContent<MarbleConnectionActivityAttributes.ContentState> {
        let expiresAt = Date.now.addingTimeInterval(25)
        return .init(
            state: .init(connectedAt: connectedAt, expiresAt: expiresAt),
            staleDate: expiresAt
        )
    }

    func endActivity() {
        activityHeartbeat?.cancel()
        activityHeartbeat = nil
        guard let activity else { return }
        self.activity = nil
        Task { await activity.end(nil, dismissalPolicy: .immediate) }
    }

    static func endActivitiesFromPreviousLaunch() {
        let activities = Activity<MarbleConnectionActivityAttributes>.activities
        Task {
            for activity in activities {
                await activity.end(nil, dismissalPolicy: .immediate)
            }
        }
    }

    static func endActivitiesBeforeTermination() {
        let activities = Activity<MarbleConnectionActivityAttributes>.activities
        guard !activities.isEmpty else { return }
        // The process can exit as soon as UIKit's termination callback
        // returns. Give ActivityKit a bounded window on a separate executor.
        let ended = DispatchSemaphore(value: 0)
        Task.detached {
            defer { ended.signal() }
            for activity in activities {
                await activity.end(nil, dismissalPolicy: .immediate)
            }
        }
        _ = ended.wait(timeout: .now() + .milliseconds(300))
    }
}
