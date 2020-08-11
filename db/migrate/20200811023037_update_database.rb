class UpdateDatabase < ActiveRecord::Migration[5.2]
  def change
    remove_column :users, :avatar_file_name
    change_column :bills, :user_id, :integer, null: false
    add_column :goals, :target_date, :datetime
    rename_column :accounts, :account_type, :category
    add_column :transactions, :tags, :string
  end
end
