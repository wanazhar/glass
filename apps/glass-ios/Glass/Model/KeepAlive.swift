import ActivityKit
import CoreLocation
import UIKit

/// Keeps Glass running in the background while it holds a live host connection, by asking iOS for the
/// coarsest location updates. Each location is dropped the moment it arrives: nothing is stored or sent.
@MainActor
final class KeepAlive: NSObject {
    static let shared = KeepAlive()
    static let key = "glass.keepAlive.v1"

    /// Location updates are running, so iOS keeps Glass awake in the background.
    private(set) var running = false
    private let manager = CLLocationManager()
    private var session: CLBackgroundActivitySession?
    private var holds: [String: @MainActor () -> Void] = [:]
    private var connectedSince: Date?
    private var activity: Activity<ConnectionActivityAttributes>?
    private var authorizing: [CheckedContinuation<Void, Never>] = []
    var hostName = "Glass host"

    var isOn: Bool { UserDefaults.standard.bool(forKey: Self.key) }
    var isConnected: Bool { !holds.isEmpty }
    private var authorized: Bool {
        [.authorizedWhenInUse, .authorizedAlways].contains(manager.authorizationStatus)
    }

    override private init() {
        super.init()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyThreeKilometers
        manager.distanceFilter = CLLocationDistanceMax
        manager.pausesLocationUpdatesAutomatically = false
        manager.activityType = .other
        let center = NotificationCenter.default
        center.addObserver(forName: .glassDisconnect, object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { KeepAlive.shared.disconnect() }
        }
        center.addObserver(forName: UIApplication.didEnterBackgroundNotification, object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { KeepAlive.shared.startActivity() }
        }
        // An activity left over from a previous run describes a connection that no longer exists.
        Task {
            for stale in Activity<ConnectionActivityAttributes>.activities {
                await stale.end(nil, dismissalPolicy: .immediate)
            }
        }
    }

    /// Turns keep-alive on or off. Turning it on asks for When In Use location; returns whether it is on.
    func set(_ on: Bool) async -> Bool {
        if on, manager.authorizationStatus == .notDetermined {
            await withCheckedContinuation { authorizing.append($0) }
        }
        let enabled = on && authorized
        UserDefaults.standard.set(enabled, forKey: Self.key)
        update()
        return enabled
    }

    /// A live connection opened. `stop` closes it when the Live Activity's Disconnect is tapped.
    func hold(_ key: String, stop: @escaping @MainActor () -> Void) {
        holds[key] = stop
        if connectedSince == nil { connectedSince = .now }
        update()
    }

    func release(_ key: String) {
        guard holds.removeValue(forKey: key) != nil else { return }
        update()
    }

    func disconnect() {
        let stops = Array(holds.values)
        holds.removeAll()
        stops.forEach { $0() }
        update()
    }

    private func update() {
        if holds.isEmpty { connectedSince = nil }
        let wanted = isOn && !holds.isEmpty && authorized
        if wanted, !running {
            manager.allowsBackgroundLocationUpdates = true
            manager.showsBackgroundLocationIndicator = true
            session = CLBackgroundActivitySession()
            manager.startUpdatingLocation()
            running = true
        } else if !wanted, running {
            manager.stopUpdatingLocation()
            session?.invalidate()
            session = nil
            manager.allowsBackgroundLocationUpdates = false
            running = false
        }
        if !wanted { endActivity() } else { refreshActivity() }
    }

    private var status: String {
        let conversations = holds.keys.filter { $0.hasPrefix("conversation:") }.count
        let desktop = holds["desktop"] != nil
        switch (conversations, desktop) {
        case (0, true): return "Desktop connected"
        case (_, true): return "Connected, with desktop"
        case (1, false): return "Connected"
        default: return "Connected to \(conversations) conversations"
        }
    }

    private func startActivity() {
        guard running, activity == nil, let connectedSince,
              ActivityAuthorizationInfo().areActivitiesEnabled else { return }
        let state = ConnectionActivityAttributes.ContentState(status: status, connectedSince: connectedSince)
        activity = try? Activity.request(attributes: ConnectionActivityAttributes(host: hostName),
                                         content: ActivityContent(state: state, staleDate: nil), pushType: nil)
    }

    private func refreshActivity() {
        guard let activity, let connectedSince else { return }
        let state = ConnectionActivityAttributes.ContentState(status: status, connectedSince: connectedSince)
        guard activity.content.state != state else { return }
        let id = activity.id
        Task.detached {
            for current in Activity<ConnectionActivityAttributes>.activities where current.id == id {
                await current.update(ActivityContent(state: state, staleDate: nil))
            }
        }
    }

    private func endActivity() {
        guard let activity else { return }
        self.activity = nil
        let id = activity.id
        Task.detached {
            for current in Activity<ConnectionActivityAttributes>.activities where current.id == id {
                await current.end(nil, dismissalPolicy: .immediate)
            }
        }
    }

    fileprivate func authorizationChanged() {
        guard manager.authorizationStatus != .notDetermined else { return }
        authorizing.forEach { $0.resume() }
        authorizing.removeAll()
        if isOn, !authorized { UserDefaults.standard.set(false, forKey: Self.key) }
        update()
    }
}

extension KeepAlive: CLLocationManagerDelegate {
    nonisolated func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {}
    nonisolated func locationManager(_ manager: CLLocationManager, didFailWithError error: any Error) {}
    nonisolated func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        Task { @MainActor in KeepAlive.shared.authorizationChanged() }
    }
}
