

export const OPEN_MODAL = 'OPEN_MODAL';
export const CLOSE_MODAL = 'CLOSE_MODAL';

export const openModal = (formType, component, payload) => ({
  type: OPEN_MODAL,
  formType,
  component,
  payload
});

export const closeModal = () => ({
  type: CLOSE_MODAL
})
