class Editaccounts < ActiveRecord::Migration[5.2]
  def change
    remove_column :accounts, :institution_id
    add_column :accounts, :debit, :boolean, null: false
    rename_column :accounts, :name, :label
  end
end
