import { NgModule } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';

/**
 * Export and Import material components
 */
const materialcomponents = [
    MatButtonModule
];

/**
 * Material Module
 */
@NgModule({
    imports: [materialcomponents],
    exports: [materialcomponents],
})
export class MaterialModule { }