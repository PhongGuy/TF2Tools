import { MatSnackBarConfig } from '@angular/material/snack-bar';

/**
 * Snack bar message
 */
export class SnackBarMessage {
    /**
     * Message  of snack bar message
     */
    message: string;
    /**
     * Action  of snack bar message
     */
    action: string = null;
    /**
     * Config  of snack bar message
     */
    config: MatSnackBarConfig = null;
}
