/**
 * Log actions
 */
export interface LogActions {
    /** What is causing the action to be triggered */
    what: 'HUDS' | 'HITSOUND' | 'CROSSHAIRS' | 'SETTINGS';
    /** What level is the log */
    level: 'INFO' | 'WARN' | 'ERROR';
    /** What did the action trigger */
    process: 'ENSURE' | 'READ' | 'COPY' | 'MOVE' | 'CHANGE' | 'RENAME' | 'DELETE' | 'WRITE' | 'REMOVE';
}
