

export const OPEN_MODAL = 'OPEN_MODAL';
export const CLOSE_MODAL = 'CLOSE_MODAL';

export const openModal = (modalType, account) => ({
  type: OPEN_MODAL,
  modalType,
  account
});

export const closeModal = () => ({
  type: CLOSE_MODAL
})
